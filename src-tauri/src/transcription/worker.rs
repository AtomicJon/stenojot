//! Background transcription worker that reads audio from ring buffers,
//! accumulates segments, and feeds them to Whisper for transcription.
//!
//! The worker runs on a dedicated thread, consuming samples from mic and
//! system ring buffer consumers. When enough audio has accumulated (or
//! silence is detected after speech), it runs Whisper inference and sends
//! the resulting `TranscriptSegment` to the frontend via a Tauri `Channel`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Instant;

use ringbuf::traits::Consumer as _;
use ringbuf::HeapCons;
use tauri::ipc::Channel;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use crate::audio::pipeline;
use crate::audio::types::{Speaker, TranscriptSegment};

/// Number of 16 kHz samples that constitute roughly 5 seconds of audio.
const SEGMENT_SAMPLES: usize = 80_000;

/// How long the worker sleeps between drain cycles (milliseconds).
const POLL_INTERVAL_MS: u64 = 100;

/// Manages a background transcription thread.
///
/// Call [`start`] to spawn the worker and [`stop`] to shut it down.
/// The worker owns the ring buffer consumers for the lifetime of a
/// recording session.
pub struct TranscriptionWorker {
    /// Shared flag the background thread checks each iteration.
    running: Arc<AtomicBool>,
    /// Join handle for the background thread.
    handle: Option<thread::JoinHandle<()>>,
}

impl TranscriptionWorker {
    /// Spawn the transcription worker thread.
    ///
    /// # Arguments
    /// * `model_path` — path to the ggml Whisper model file
    /// * `mic_consumer` — ring buffer consumer for the microphone stream
    /// * `system_consumer` — ring buffer consumer for the system audio stream
    /// * `mic_sample_rate` — sample rate of the mic capture
    /// * `mic_channels` — channel count of the mic capture
    /// * `system_sample_rate` — sample rate of the system capture
    /// * `system_channels` — channel count of the system capture
    /// * `channel` — Tauri IPC channel for sending segments to the frontend
    pub fn start(
        model_path: std::path::PathBuf,
        mic_consumer: HeapCons<f32>,
        system_consumer: HeapCons<f32>,
        mic_sample_rate: u32,
        mic_channels: u16,
        system_sample_rate: u32,
        system_channels: u16,
        channel: Channel<TranscriptSegment>,
    ) -> Result<Self, String> {
        let running = Arc::new(AtomicBool::new(true));
        let running_clone = Arc::clone(&running);

        let handle = thread::spawn(move || {
            worker_loop(
                running_clone,
                model_path,
                mic_consumer,
                system_consumer,
                mic_sample_rate,
                mic_channels,
                system_sample_rate,
                system_channels,
                channel,
            );
        });

        Ok(Self {
            running,
            handle: Some(handle),
        })
    }

    /// Signal the worker to stop and wait for the thread to finish.
    pub fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

/// Main loop executed on the worker thread.
fn worker_loop(
    running: Arc<AtomicBool>,
    model_path: std::path::PathBuf,
    mut mic_consumer: HeapCons<f32>,
    mut system_consumer: HeapCons<f32>,
    mic_sample_rate: u32,
    mic_channels: u16,
    system_sample_rate: u32,
    system_channels: u16,
    channel: Channel<TranscriptSegment>,
) {
    // Load the Whisper model once for the lifetime of the worker
    let ctx = match WhisperContext::new_with_params(
        model_path.to_str().unwrap_or_default(),
        WhisperContextParameters::default(),
    ) {
        Ok(ctx) => ctx,
        Err(e) => {
            eprintln!("Failed to load Whisper model: {}", e);
            return;
        }
    };

    let recording_start = Instant::now();

    // Accumulated raw samples (at source rate/channels) for each stream
    let mut mic_raw_buf: Vec<f32> = Vec::new();
    let mut system_raw_buf: Vec<f32> = Vec::new();

    // Track segment start times in milliseconds
    let mut mic_segment_start_ms: u64 = 0;
    let mut system_segment_start_ms: u64 = 0;

    // Track whether we've seen speech in the current segment
    let mut mic_had_speech = false;
    let mut system_had_speech = false;

    while running.load(Ordering::SeqCst) {
        // Drain mic ring buffer
        drain_consumer(&mut mic_consumer, &mut mic_raw_buf);

        // Drain system ring buffer
        drain_consumer(&mut system_consumer, &mut system_raw_buf);

        // Process mic segment
        let mic_mono = pipeline::process_buffer(&mic_raw_buf, mic_sample_rate, mic_channels);
        if should_transcribe(&mic_mono, &mut mic_had_speech) {
            let elapsed_ms = recording_start.elapsed().as_millis() as u64;
            transcribe_segment(
                &ctx,
                &mic_mono,
                Speaker::Me,
                mic_segment_start_ms,
                elapsed_ms,
                &channel,
            );
            mic_raw_buf.clear();
            mic_segment_start_ms = elapsed_ms;
            mic_had_speech = false;
        }

        // Process system segment
        let system_mono =
            pipeline::process_buffer(&system_raw_buf, system_sample_rate, system_channels);
        if should_transcribe(&system_mono, &mut system_had_speech) {
            let elapsed_ms = recording_start.elapsed().as_millis() as u64;
            transcribe_segment(
                &ctx,
                &system_mono,
                Speaker::Others,
                system_segment_start_ms,
                elapsed_ms,
                &channel,
            );
            system_raw_buf.clear();
            system_segment_start_ms = elapsed_ms;
            system_had_speech = false;
        }

        thread::sleep(std::time::Duration::from_millis(POLL_INTERVAL_MS));
    }

    // Flush any remaining audio in both buffers
    let elapsed_ms = recording_start.elapsed().as_millis() as u64;

    let mic_mono = pipeline::process_buffer(&mic_raw_buf, mic_sample_rate, mic_channels);
    if !mic_mono.is_empty() && pipeline::is_speech(&mic_mono) {
        transcribe_segment(
            &ctx,
            &mic_mono,
            Speaker::Me,
            mic_segment_start_ms,
            elapsed_ms,
            &channel,
        );
    }

    let system_mono =
        pipeline::process_buffer(&system_raw_buf, system_sample_rate, system_channels);
    if !system_mono.is_empty() && pipeline::is_speech(&system_mono) {
        transcribe_segment(
            &ctx,
            &system_mono,
            Speaker::Others,
            system_segment_start_ms,
            elapsed_ms,
            &channel,
        );
    }
}

/// Pop all available samples from a ring buffer consumer into the accumulation buffer.
fn drain_consumer(consumer: &mut HeapCons<f32>, buf: &mut Vec<f32>) {
    while let Some(sample) = consumer.try_pop() {
        buf.push(sample);
    }
}

/// Decide whether the accumulated 16 kHz mono buffer is ready to transcribe.
///
/// Triggers when the buffer reaches the segment length threshold, or when
/// silence is detected after a period of speech (allowing shorter natural
/// segments).
fn should_transcribe(mono_16k: &[f32], had_speech: &mut bool) -> bool {
    if mono_16k.len() >= SEGMENT_SAMPLES {
        return true;
    }

    let has_speech_now = pipeline::is_speech(mono_16k);
    if has_speech_now {
        *had_speech = true;
    }

    // Silence after speech with a reasonable minimum length (~1 second)
    if *had_speech && !has_speech_now && mono_16k.len() >= 16_000 {
        return true;
    }

    false
}

/// Run Whisper inference on a 16 kHz mono audio segment and send the result
/// to the frontend.
fn transcribe_segment(
    ctx: &WhisperContext,
    audio: &[f32],
    speaker: Speaker,
    start_ms: u64,
    end_ms: u64,
    channel: &Channel<TranscriptSegment>,
) {
    let mut state = match ctx.create_state() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to create Whisper state: {}", e);
            return;
        }
    };

    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_language(Some("en"));
    params.set_print_special(false);
    params.set_print_realtime(false);
    params.set_print_progress(false);

    if let Err(e) = state.full(params, audio) {
        eprintln!("Whisper inference failed: {}", e);
        return;
    }

    let num_segments = state.full_n_segments();
    let mut text_parts: Vec<String> = Vec::new();

    for i in 0..num_segments {
        if let Some(seg) = state.get_segment(i) {
            if let Ok(segment_text) = seg.to_str_lossy() {
                let trimmed = segment_text.trim().to_string();
                if !trimmed.is_empty() {
                    text_parts.push(trimmed);
                }
            }
        }
    }

    let text = text_parts.join(" ");
    if text.is_empty() {
        return;
    }

    let segment = TranscriptSegment {
        text,
        speaker,
        start_ms,
        end_ms,
        is_final: true,
    };

    channel.send(segment).ok();
}
