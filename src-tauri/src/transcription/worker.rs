//! Background transcription worker that reads audio from ring buffers,
//! accumulates segments, and feeds them to Whisper for transcription.
//!
//! The worker runs on a dedicated thread, consuming samples from mic and
//! system ring buffer consumers. When enough audio has accumulated (or
//! silence is detected after speech), it runs Whisper inference and sends
//! the resulting `TranscriptSegment` to the frontend via a Tauri `Channel`.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
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
    /// When true, audio is drained but discarded (not accumulated).
    paused: Arc<AtomicBool>,
    /// Set to true by the worker when it auto-stops due to silence timeout.
    auto_stopped: Arc<AtomicBool>,
    /// Shared segment accumulator — worker pushes, commands read for periodic saves.
    shared_segments: Arc<Mutex<Vec<TranscriptSegment>>>,
    /// Join handle for the background thread. Returns accumulated segments.
    handle: Option<thread::JoinHandle<Vec<TranscriptSegment>>>,
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
        vad_threshold: Arc<AtomicU32>,
        silence_timeout_seconds: Option<u32>,
        channel: Channel<TranscriptSegment>,
    ) -> Result<Self, String> {
        let running = Arc::new(AtomicBool::new(true));
        let running_clone = Arc::clone(&running);
        let paused = Arc::new(AtomicBool::new(false));
        let paused_clone = Arc::clone(&paused);
        let auto_stopped = Arc::new(AtomicBool::new(false));
        let auto_stopped_clone = Arc::clone(&auto_stopped);
        let shared_segments: Arc<Mutex<Vec<TranscriptSegment>>> =
            Arc::new(Mutex::new(Vec::new()));
        let shared_segments_clone = Arc::clone(&shared_segments);

        let handle = thread::spawn(move || {
            worker_loop(
                running_clone,
                paused_clone,
                auto_stopped_clone,
                shared_segments_clone,
                model_path,
                mic_consumer,
                system_consumer,
                mic_sample_rate,
                mic_channels,
                system_sample_rate,
                system_channels,
                vad_threshold,
                silence_timeout_seconds,
                channel,
            )
        });

        Ok(Self {
            running,
            paused,
            auto_stopped,
            shared_segments,
            handle: Some(handle),
        })
    }

    /// Pause the worker — audio is still drained but discarded.
    pub fn pause(&self) {
        self.paused.store(true, Ordering::SeqCst);
    }

    /// Resume the worker after a pause.
    pub fn resume(&self) {
        self.paused.store(false, Ordering::SeqCst);
    }

    /// Check whether the worker is currently paused.
    #[allow(dead_code)]
    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::SeqCst)
    }

    /// Check whether the worker auto-stopped due to silence timeout.
    pub fn auto_stopped(&self) -> bool {
        self.auto_stopped.load(Ordering::SeqCst)
    }

    /// Get a snapshot of all accumulated segments so far.
    pub fn get_segments(&self) -> Vec<TranscriptSegment> {
        self.shared_segments
            .lock()
            .map(|s| s.clone())
            .unwrap_or_default()
    }

    /// Signal the worker to stop, wait for the thread to finish, and return
    /// all accumulated transcript segments from the recording session.
    pub fn stop(&mut self) -> Vec<TranscriptSegment> {
        self.running.store(false, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            handle.join().unwrap_or_default()
        } else {
            Vec::new()
        }
    }
}

/// Main loop executed on the worker thread.
///
/// Returns all transcript segments accumulated during the recording session.
fn worker_loop(
    running: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
    auto_stopped: Arc<AtomicBool>,
    shared_segments: Arc<Mutex<Vec<TranscriptSegment>>>,
    model_path: std::path::PathBuf,
    mut mic_consumer: HeapCons<f32>,
    mut system_consumer: HeapCons<f32>,
    mic_sample_rate: u32,
    mic_channels: u16,
    system_sample_rate: u32,
    system_channels: u16,
    vad_threshold: Arc<AtomicU32>,
    silence_timeout_seconds: Option<u32>,
    channel: Channel<TranscriptSegment>,
) -> Vec<TranscriptSegment> {
    // Load the Whisper model once for the lifetime of the worker
    let ctx = match WhisperContext::new_with_params(
        model_path.to_str().unwrap_or_default(),
        WhisperContextParameters::default(),
    ) {
        Ok(ctx) => ctx,
        Err(e) => {
            eprintln!("Failed to load Whisper model: {}", e);
            return Vec::new();
        }
    };

    let recording_start = Instant::now();
    let mut all_segments: Vec<TranscriptSegment> = Vec::new();
    let mut last_speech_time = Instant::now();

    // Helper: push a segment to both local and shared accumulators
    let push_segment = |seg: TranscriptSegment,
                            all: &mut Vec<TranscriptSegment>,
                            shared: &Arc<Mutex<Vec<TranscriptSegment>>>| {
        if let Ok(mut shared_segs) = shared.lock() {
            shared_segs.push(seg.clone());
        }
        all.push(seg);
    };

    // Accumulated raw samples (at source rate/channels) for each stream
    let mut mic_raw_buf: Vec<f32> = Vec::new();
    let mut system_raw_buf: Vec<f32> = Vec::new();

    // Track segment start times in milliseconds
    let mut mic_segment_start_ms: u64 = 0;
    let mut system_segment_start_ms: u64 = 0;

    // Track whether we've seen speech in the current segment
    let mut mic_had_speech = false;
    let mut system_had_speech = false;
    let mut mic_silence_count: u32 = 0;
    let mut system_silence_count: u32 = 0;

    // Scratch buffer for draining when paused
    let mut drain_scratch: Vec<f32> = Vec::new();

    while running.load(Ordering::SeqCst) {
        // When paused, drain ring buffers to prevent overflow but discard samples
        if paused.load(Ordering::SeqCst) {
            drain_consumer(&mut mic_consumer, &mut drain_scratch);
            drain_consumer(&mut system_consumer, &mut drain_scratch);
            drain_scratch.clear();
            // Reset silence timeout clock while paused
            last_speech_time = Instant::now();
            thread::sleep(std::time::Duration::from_millis(POLL_INTERVAL_MS));
            continue;
        }

        let threshold = f32::from_bits(vad_threshold.load(Ordering::Relaxed));

        // Drain mic ring buffer
        drain_consumer(&mut mic_consumer, &mut mic_raw_buf);

        // Drain system ring buffer
        drain_consumer(&mut system_consumer, &mut system_raw_buf);

        // Process mic segment
        let mic_mono = pipeline::process_buffer(&mic_raw_buf, mic_sample_rate, mic_channels);
        if should_transcribe(&mic_mono, &mut mic_had_speech, &mut mic_silence_count, threshold) {
            let elapsed_ms = recording_start.elapsed().as_millis() as u64;
            if mic_had_speech {
                if let Some(seg) = transcribe_segment(
                    &ctx, &mic_mono, Speaker::Me, mic_segment_start_ms, elapsed_ms, &channel,
                ) {
                    push_segment(seg, &mut all_segments, &shared_segments);
                }
            }
            mic_raw_buf.clear();
            mic_segment_start_ms = elapsed_ms;
            mic_had_speech = false;
            mic_silence_count = 0;
        }

        // Process system segment
        let system_mono =
            pipeline::process_buffer(&system_raw_buf, system_sample_rate, system_channels);
        if should_transcribe(&system_mono, &mut system_had_speech, &mut system_silence_count, threshold) {
            let elapsed_ms = recording_start.elapsed().as_millis() as u64;
            if system_had_speech {
                if let Some(seg) = transcribe_segment(
                    &ctx, &system_mono, Speaker::Others, system_segment_start_ms, elapsed_ms, &channel,
                ) {
                    push_segment(seg, &mut all_segments, &shared_segments);
                }
            }
            system_raw_buf.clear();
            system_segment_start_ms = elapsed_ms;
            system_had_speech = false;
            system_silence_count = 0;
        }

        // Track last speech time for silence timeout
        if mic_had_speech || system_had_speech {
            last_speech_time = Instant::now();
        }

        // Auto-stop if silence exceeds the configured timeout
        if let Some(timeout_secs) = silence_timeout_seconds {
            if timeout_secs > 0
                && last_speech_time.elapsed().as_secs() >= u64::from(timeout_secs)
            {
                eprintln!(
                    "Auto-stopping: no speech for {} seconds",
                    timeout_secs
                );
                auto_stopped.store(true, Ordering::SeqCst);
                running.store(false, Ordering::SeqCst);
            }
        }

        thread::sleep(std::time::Duration::from_millis(POLL_INTERVAL_MS));
    }

    // Flush any remaining audio in both buffers
    let elapsed_ms = recording_start.elapsed().as_millis() as u64;
    let threshold = f32::from_bits(vad_threshold.load(Ordering::Relaxed));

    let mic_mono = pipeline::process_buffer(&mic_raw_buf, mic_sample_rate, mic_channels);
    if !mic_mono.is_empty() && pipeline::is_speech(&mic_mono, threshold) {
        if let Some(seg) = transcribe_segment(
            &ctx, &mic_mono, Speaker::Me, mic_segment_start_ms, elapsed_ms, &channel,
        ) {
            push_segment(seg, &mut all_segments, &shared_segments);
        }
    }

    let system_mono =
        pipeline::process_buffer(&system_raw_buf, system_sample_rate, system_channels);
    if !system_mono.is_empty() && pipeline::is_speech(&system_mono, threshold) {
        if let Some(seg) = transcribe_segment(
            &ctx, &system_mono, Speaker::Others, system_segment_start_ms, elapsed_ms, &channel,
        ) {
            push_segment(seg, &mut all_segments, &shared_segments);
        }
    }

    all_segments
}

/// Pop all available samples from a ring buffer consumer into the accumulation buffer.
fn drain_consumer(consumer: &mut HeapCons<f32>, buf: &mut Vec<f32>) {
    while let Some(sample) = consumer.try_pop() {
        buf.push(sample);
    }
}

/// Size of the trailing window used for VAD checks (~100ms at 16 kHz).
const VAD_WINDOW: usize = 1_600;

/// Minimum buffer length before we'll send a segment for transcription
/// (~0.25 seconds at 16 kHz). Short words like "yes" can be under 0.3s,
/// so we keep this low and let Whisper handle the decoding.
const MIN_SEGMENT_SAMPLES: usize = 4_000;

/// Number of consecutive silent polls required before cutting a segment.
/// At 100ms poll interval, 5 polls = 500ms of silence before we decide
/// the speaker has stopped. This prevents cutting mid-word on brief pauses.
const SILENCE_HOLDOFF: u32 = 5;

/// Decide whether the accumulated 16 kHz mono buffer is ready to transcribe.
///
/// Uses a trailing window for VAD so short utterances are detected even
/// when surrounded by silence. A silence holdoff counter prevents cutting
/// segments on brief pauses between words.
///
/// Triggers on:
/// 1. Buffer reaching the max segment length (~5 seconds).
/// 2. Sustained silence after speech (holdoff expired, minimum buffer met).
fn should_transcribe(
    mono_16k: &[f32],
    had_speech: &mut bool,
    silence_count: &mut u32,
    threshold: f32,
) -> bool {
    if mono_16k.len() >= SEGMENT_SAMPLES {
        return true;
    }

    // Check only the trailing window for speech, not the entire buffer.
    // This prevents short words from being diluted by surrounding silence.
    let window_start = mono_16k.len().saturating_sub(VAD_WINDOW);
    let has_speech_now = pipeline::is_speech(&mono_16k[window_start..], threshold);

    if has_speech_now {
        *had_speech = true;
        *silence_count = 0;
    } else if *had_speech {
        *silence_count += 1;
    }

    // Only cut the segment after sustained silence following speech
    if *had_speech && *silence_count >= SILENCE_HOLDOFF && mono_16k.len() >= MIN_SEGMENT_SAMPLES {
        return true;
    }

    false
}

/// Check whether Whisper output is a non-speech artifact (silence markers,
/// blank audio tags, music notes, etc.) that should be discarded.
fn is_non_speech(text: &str) -> bool {
    let lower = text.trim().to_lowercase();
    // Strip surrounding brackets/parens for matching
    let inner = lower
        .trim_start_matches(['[', '('])
        .trim_end_matches([']', ')'])
        .trim();

    matches!(
        inner,
        "silence"
            | "blank_audio"
            | "blank audio"
            | "no speech"
            | "no speech detected"
            | "inaudible"
            | "background noise"
    ) || inner.contains("blank_audio")
        || inner.contains("no speech")
        // Whisper sometimes outputs musical note characters for non-speech audio
        || lower.chars().all(|c| c == '♪' || c == '♫' || c.is_whitespace())
}

/// Run Whisper inference on a 16 kHz mono audio segment, send the result
/// to the frontend, and return a clone for accumulation.
fn transcribe_segment(
    ctx: &WhisperContext,
    audio: &[f32],
    speaker: Speaker,
    start_ms: u64,
    end_ms: u64,
    channel: &Channel<TranscriptSegment>,
) -> Option<TranscriptSegment> {
    let mut state = match ctx.create_state() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to create Whisper state: {}", e);
            return None;
        }
    };

    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_language(Some("en"));
    params.set_print_special(false);
    params.set_print_realtime(false);
    params.set_print_progress(false);

    if let Err(e) = state.full(params, audio) {
        eprintln!("Whisper inference failed: {}", e);
        return None;
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
    if text.is_empty() || is_non_speech(&text) {
        return None;
    }

    let segment = TranscriptSegment {
        text,
        speaker,
        start_ms,
        end_ms,
        is_final: true,
    };

    channel.send(segment.clone()).ok();
    Some(segment)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── is_non_speech ────────────────────────────────

    #[test]
    fn is_non_speech_detects_bracketed_silence() {
        // Arrange
        let text = "[silence]";

        // Act
        let result = is_non_speech(text);

        // Assert
        assert!(result);
    }

    #[test]
    fn is_non_speech_detects_blank_audio() {
        // Arrange
        let text = "[BLANK_AUDIO]";

        // Act
        let result = is_non_speech(text);

        // Assert
        assert!(result);
    }

    #[test]
    fn is_non_speech_detects_parenthesized_no_speech() {
        // Arrange
        let text = "(no speech detected)";

        // Act
        let result = is_non_speech(text);

        // Assert
        assert!(result);
    }

    #[test]
    fn is_non_speech_detects_music_notes() {
        // Arrange
        let text = "♪ ♫ ♪";

        // Act
        let result = is_non_speech(text);

        // Assert
        assert!(result);
    }

    #[test]
    fn is_non_speech_detects_inaudible() {
        // Arrange
        let text = "[inaudible]";

        // Act
        let result = is_non_speech(text);

        // Assert
        assert!(result);
    }

    #[test]
    fn is_non_speech_detects_background_noise() {
        // Arrange
        let text = "(background noise)";

        // Act
        let result = is_non_speech(text);

        // Assert
        assert!(result);
    }

    #[test]
    fn is_non_speech_allows_real_speech() {
        // Arrange
        let text = "Hello, how are you?";

        // Act
        let result = is_non_speech(text);

        // Assert
        assert!(!result);
    }

    #[test]
    fn is_non_speech_allows_short_words() {
        // Arrange
        let text = "Yes";

        // Act
        let result = is_non_speech(text);

        // Assert
        assert!(!result);
    }

    #[test]
    fn is_non_speech_handles_whitespace_padding() {
        // Arrange
        let text = "  [silence]  ";

        // Act
        let result = is_non_speech(text);

        // Assert
        assert!(result);
    }

    // ── should_transcribe ────────────────────────────

    #[test]
    fn should_transcribe_triggers_on_max_segment_length() {
        // Arrange — buffer at exactly SEGMENT_SAMPLES (80,000)
        let mono: Vec<f32> = vec![0.0; SEGMENT_SAMPLES];
        let mut had_speech = false;
        let mut silence_count = 0u32;
        let threshold = 0.01;

        // Act
        let result = should_transcribe(&mono, &mut had_speech, &mut silence_count, threshold);

        // Assert
        assert!(result);
    }

    #[test]
    fn should_transcribe_does_not_trigger_on_silence_only() {
        // Arrange — short silent buffer, no prior speech
        let mono: Vec<f32> = vec![0.0; 8000];
        let mut had_speech = false;
        let mut silence_count = 0u32;
        let threshold = 0.01;

        // Act
        let result = should_transcribe(&mono, &mut had_speech, &mut silence_count, threshold);

        // Assert
        assert!(!result);
        assert!(!had_speech);
    }

    #[test]
    fn should_transcribe_detects_speech_in_trailing_window() {
        // Arrange — silence followed by a loud trailing window
        let mut mono: Vec<f32> = vec![0.0; 8000];
        // Fill the last VAD_WINDOW samples with speech
        let speech_start = mono.len() - VAD_WINDOW;
        for sample in &mut mono[speech_start..] {
            *sample = 0.5;
        }
        let mut had_speech = false;
        let mut silence_count = 0u32;
        let threshold = 0.01;

        // Act
        let result = should_transcribe(&mono, &mut had_speech, &mut silence_count, threshold);

        // Assert — speech detected but holdoff not yet expired
        assert!(!result);
        assert!(had_speech);
        assert_eq!(silence_count, 0);
    }

    #[test]
    fn should_transcribe_triggers_after_silence_holdoff() {
        // Arrange — simulate speech detected, then enough silence polls
        let mono: Vec<f32> = vec![0.0; MIN_SEGMENT_SAMPLES]; // silent buffer
        let mut had_speech = true; // speech was detected previously
        let mut silence_count = SILENCE_HOLDOFF - 1; // one poll away from triggering
        let threshold = 0.01;

        // Act — this poll should push silence_count to SILENCE_HOLDOFF
        let result = should_transcribe(&mono, &mut had_speech, &mut silence_count, threshold);

        // Assert
        assert!(result);
        assert_eq!(silence_count, SILENCE_HOLDOFF);
    }

    #[test]
    fn should_transcribe_resets_silence_count_on_new_speech() {
        // Arrange — speech was detected, some silence accumulated, then speech again
        let mut mono: Vec<f32> = vec![0.0; 8000];
        let speech_start = mono.len() - VAD_WINDOW;
        for sample in &mut mono[speech_start..] {
            *sample = 0.5;
        }
        let mut had_speech = true;
        let mut silence_count = 3u32;
        let threshold = 0.01;

        // Act
        let _result = should_transcribe(&mono, &mut had_speech, &mut silence_count, threshold);

        // Assert — silence count reset because speech was detected
        assert_eq!(silence_count, 0);
    }

    #[test]
    fn should_transcribe_does_not_trigger_below_min_segment() {
        // Arrange — had speech + holdoff expired but buffer too short
        let mono: Vec<f32> = vec![0.0; MIN_SEGMENT_SAMPLES - 1];
        let mut had_speech = true;
        let mut silence_count = SILENCE_HOLDOFF - 1;
        let threshold = 0.01;

        // Act
        let result = should_transcribe(&mono, &mut had_speech, &mut silence_count, threshold);

        // Assert — holdoff reached but buffer too short
        assert!(!result);
    }
}
