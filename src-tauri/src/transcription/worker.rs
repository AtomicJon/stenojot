//! Background transcription worker that reads audio from ring buffers,
//! accumulates segments, and feeds them to the selected STT engine.
//!
//! The worker runs on a dedicated thread, consuming samples from mic and
//! system ring buffer consumers. When enough audio has accumulated (or
//! silence is detected after speech), it runs inference via the configured
//! [`SttBackend`](super::engine::SttBackend) and sends the resulting
//! `TranscriptSegment` to the frontend via a Tauri `Channel`.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;

use ringbuf::traits::Consumer as _;
use ringbuf::HeapCons;
use tauri::ipc::Channel;

use crate::audio::pipeline;
use crate::audio::types::{Speaker, TranscriptSegment};
use crate::audio::vad::{self, Segmenter, SegmenterConfig, VadKind};

use super::engine::{self, SttBackend, SttEngine};

/// Configuration for starting a transcription worker.
///
/// Bundles all the parameters needed to spawn the background transcription
/// thread, keeping function signatures manageable.
pub struct WorkerConfig {
    /// The STT engine to use for transcription.
    pub engine: SttEngine,
    /// Path to the model file (Whisper GGML) or directory (ONNX models).
    pub model_path: std::path::PathBuf,
    /// Ring buffer consumer for the microphone stream.
    pub mic_consumer: HeapCons<f32>,
    /// Ring buffer consumer for the system audio stream.
    pub system_consumer: HeapCons<f32>,
    /// Sample rate of the mic capture.
    pub mic_sample_rate: u32,
    /// Channel count of the mic capture.
    pub mic_channels: u16,
    /// Sample rate of the system capture.
    pub system_sample_rate: u32,
    /// Channel count of the system capture.
    pub system_channels: u16,
    /// Shared VAD threshold value (used only by the energy VAD backend).
    pub vad_threshold: Arc<AtomicU32>,
    /// Which VAD backend to use for speech segmentation.
    pub vad_kind: VadKind,
    /// Auto-stop after this many seconds of silence.
    pub silence_timeout_seconds: Option<u32>,
    /// Optional prompt to guide transcription (domain terms, names).
    /// Supported by Whisper; other engines may ignore it.
    pub initial_prompt: Option<String>,
    /// Maximum segment duration before forced transcription.
    pub max_segment_seconds: u32,
    /// Tauri IPC channel for sending segments to the frontend.
    pub channel: Channel<TranscriptSegment>,
}

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
    /// * `config` — all configuration needed to run the worker (see [`WorkerConfig`])
    pub fn start(config: WorkerConfig) -> Result<Self, String> {
        let running = Arc::new(AtomicBool::new(true));
        let running_clone = Arc::clone(&running);
        let paused = Arc::new(AtomicBool::new(false));
        let paused_clone = Arc::clone(&paused);
        let auto_stopped = Arc::new(AtomicBool::new(false));
        let auto_stopped_clone = Arc::clone(&auto_stopped);
        let shared_segments: Arc<Mutex<Vec<TranscriptSegment>>> = Arc::new(Mutex::new(Vec::new()));
        let shared_segments_clone = Arc::clone(&shared_segments);

        let handle = thread::spawn(move || {
            worker_loop(
                running_clone,
                paused_clone,
                auto_stopped_clone,
                shared_segments_clone,
                config,
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
    ///
    /// If the worker thread previously panicked while holding the segment
    /// mutex, the lock will be poisoned. We recover the inner data instead
    /// of silently returning an empty vector so callers still see whatever
    /// was successfully transcribed before the panic.
    pub fn get_segments(&self) -> Vec<TranscriptSegment> {
        match self.shared_segments.lock() {
            Ok(guard) => guard.clone(),
            Err(poisoned) => {
                eprintln!("transcription worker segment mutex was poisoned");
                poisoned.into_inner().clone()
            }
        }
    }

    /// Signal the worker to stop, wait for the thread to finish, and return
    /// all accumulated transcript segments from the recording session.
    ///
    /// If the worker thread panicked, the panic payload is logged and an
    /// empty vector is returned (we cannot recover segments from a panicked
    /// `JoinHandle`'s return value).
    pub fn stop(&mut self) -> Vec<TranscriptSegment> {
        // `Release` is enough — the worker side reads with `Acquire`/`Relaxed`
        // and we're not synchronising any other data through this flag.
        self.running.store(false, Ordering::Release);
        if let Some(handle) = self.handle.take() {
            match handle.join() {
                Ok(segments) => segments,
                Err(panic_payload) => {
                    eprintln!("transcription worker thread panicked: {panic_payload:?}");
                    Vec::new()
                }
            }
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
    config: WorkerConfig,
) -> Vec<TranscriptSegment> {
    let WorkerConfig {
        engine,
        model_path,
        mut mic_consumer,
        mut system_consumer,
        mic_sample_rate,
        mic_channels,
        system_sample_rate,
        system_channels,
        vad_threshold,
        vad_kind,
        silence_timeout_seconds,
        initial_prompt,
        max_segment_seconds,
        channel,
    } = config;

    // Load the STT backend once for the lifetime of the worker
    let mut backend = match engine::create_backend(engine, &model_path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("Failed to load {} model: {}", engine, e);
            return Vec::new();
        }
    };

    // Build one segmenter per stream. Each owns its own VAD instance because
    // the neural backends carry recurrent state that must not be shared.
    let energy_threshold = f32::from_bits(vad_threshold.load(Ordering::Relaxed));
    let seg_config = SegmenterConfig {
        max_segment_ms: max_segment_seconds.clamp(1, 30) * 1_000,
        ..Default::default()
    };
    let mut mic_segmenter = build_segmenter(vad_kind, energy_threshold, seg_config);
    let mut system_segmenter = build_segmenter(vad_kind, energy_threshold, seg_config);

    let recording_start = Instant::now();
    let mut all_segments: Vec<TranscriptSegment> = Vec::new();
    let mut last_speech_time = Instant::now();

    // Helper: push a segment to both local and shared accumulators
    let push_segment = |seg: TranscriptSegment,
                        all: &mut Vec<TranscriptSegment>,
                        shared: &Arc<Mutex<Vec<TranscriptSegment>>>| {
        // Recover from poisoning rather than silently dropping segments —
        // see `TranscriptionWorker::get_segments` for the rationale.
        match shared.lock() {
            Ok(mut shared_segs) => shared_segs.push(seg.clone()),
            Err(poisoned) => poisoned.into_inner().push(seg.clone()),
        }
        all.push(seg);
    };

    // Reused buffers for draining the ring buffers each poll.
    let mut mic_raw_buf: Vec<f32> = Vec::new();
    let mut system_raw_buf: Vec<f32> = Vec::new();

    while running.load(Ordering::SeqCst) {
        // When paused, drain ring buffers to prevent overflow but discard
        // samples and reset the segmenters so partial speech isn't stitched
        // across the pause.
        if paused.load(Ordering::SeqCst) {
            drain_consumer(&mut mic_consumer, &mut mic_raw_buf);
            drain_consumer(&mut system_consumer, &mut system_raw_buf);
            mic_raw_buf.clear();
            system_raw_buf.clear();
            mic_segmenter.reset();
            system_segmenter.reset();
            last_speech_time = Instant::now();
            thread::sleep(std::time::Duration::from_millis(POLL_INTERVAL_MS));
            continue;
        }

        // Drain, resample to 16 kHz mono, and feed each stream's segmenter.
        drain_consumer(&mut mic_consumer, &mut mic_raw_buf);
        drain_consumer(&mut system_consumer, &mut system_raw_buf);

        let mic_mono = pipeline::process_buffer(&mic_raw_buf, mic_sample_rate, mic_channels);
        mic_raw_buf.clear();
        let mic_ready = mic_segmenter.push_samples(&mic_mono);

        let system_mono =
            pipeline::process_buffer(&system_raw_buf, system_sample_rate, system_channels);
        system_raw_buf.clear();
        let system_ready = system_segmenter.push_samples(&system_mono);

        let any_speech = !mic_ready.is_empty() || !system_ready.is_empty();

        emit_segments(
            mic_ready,
            Speaker::Me,
            recording_start,
            &mut *backend,
            initial_prompt.as_deref(),
            &channel,
            &push_segment,
            &mut all_segments,
            &shared_segments,
        );
        emit_segments(
            system_ready,
            Speaker::Others,
            recording_start,
            &mut *backend,
            initial_prompt.as_deref(),
            &channel,
            &push_segment,
            &mut all_segments,
            &shared_segments,
        );

        if any_speech {
            last_speech_time = Instant::now();
        }

        // Auto-stop if silence exceeds the configured timeout
        if let Some(timeout_secs) = silence_timeout_seconds {
            if timeout_secs > 0 && last_speech_time.elapsed().as_secs() >= u64::from(timeout_secs) {
                eprintln!("Auto-stopping: no speech for {} seconds", timeout_secs);
                auto_stopped.store(true, Ordering::SeqCst);
                running.store(false, Ordering::SeqCst);
            }
        }

        thread::sleep(std::time::Duration::from_millis(POLL_INTERVAL_MS));
    }

    // Final drain: capture any samples pushed since the last loop iteration,
    // feed them through, then flush any in-progress speech segment.
    drain_consumer(&mut mic_consumer, &mut mic_raw_buf);
    drain_consumer(&mut system_consumer, &mut system_raw_buf);

    let mic_mono = pipeline::process_buffer(&mic_raw_buf, mic_sample_rate, mic_channels);
    let mut mic_final = mic_segmenter.push_samples(&mic_mono);
    mic_final.extend(mic_segmenter.flush());
    emit_segments(
        mic_final,
        Speaker::Me,
        recording_start,
        &mut *backend,
        initial_prompt.as_deref(),
        &channel,
        &push_segment,
        &mut all_segments,
        &shared_segments,
    );

    let system_mono =
        pipeline::process_buffer(&system_raw_buf, system_sample_rate, system_channels);
    let mut system_final = system_segmenter.push_samples(&system_mono);
    system_final.extend(system_segmenter.flush());
    emit_segments(
        system_final,
        Speaker::Others,
        recording_start,
        &mut *backend,
        initial_prompt.as_deref(),
        &channel,
        &push_segment,
        &mut all_segments,
        &shared_segments,
    );

    all_segments
}

/// Pop all available samples from a ring buffer consumer into the accumulation buffer.
fn drain_consumer(consumer: &mut HeapCons<f32>, buf: &mut Vec<f32>) {
    while let Some(sample) = consumer.try_pop() {
        buf.push(sample);
    }
}

/// Build a [`Segmenter`] for one audio stream using the selected VAD backend.
///
/// If the selected (neural) backend fails to load at runtime — e.g. the ONNX
/// Runtime library can't be located — we fall back to the always-available
/// energy VAD rather than aborting the whole recording, so transcription keeps
/// working (just with the basic detector). The failure is logged so the cause
/// is visible.
fn build_segmenter(kind: VadKind, energy_threshold: f32, config: SegmenterConfig) -> Segmenter {
    let detector = match vad::create_vad(kind, energy_threshold) {
        Ok(d) => {
            eprintln!("[vad] backend active: {kind}");
            d
        }
        Err(e) => {
            eprintln!("[vad] backend '{kind}' failed to load: {e}; falling back to energy VAD");
            vad::create_vad(VadKind::Energy, energy_threshold)
                .expect("energy VAD construction never fails")
        }
    };
    Segmenter::new(detector, config)
}

/// Convert a count of 16 kHz samples to milliseconds.
fn samples_to_ms(samples: usize) -> u64 {
    (samples as u64) * 1_000 / u64::from(vad::VAD_SAMPLE_RATE)
}

/// Transcribe each finalized segment and push the results to the accumulators.
///
/// Each segment's `end_ms` is the current elapsed recording time; `start_ms`
/// is derived by subtracting the segment's audio length.
#[allow(clippy::too_many_arguments)]
fn emit_segments<F>(
    segments: Vec<Vec<f32>>,
    speaker: Speaker,
    recording_start: Instant,
    backend: &mut dyn SttBackend,
    initial_prompt: Option<&str>,
    channel: &Channel<TranscriptSegment>,
    push_segment: &F,
    all: &mut Vec<TranscriptSegment>,
    shared: &Arc<Mutex<Vec<TranscriptSegment>>>,
) where
    F: Fn(TranscriptSegment, &mut Vec<TranscriptSegment>, &Arc<Mutex<Vec<TranscriptSegment>>>),
{
    for audio in segments {
        let end_ms = recording_start.elapsed().as_millis() as u64;
        let start_ms = end_ms.saturating_sub(samples_to_ms(audio.len()));
        if let Some(seg) = transcribe_segment(
            backend,
            &audio,
            speaker,
            start_ms,
            end_ms,
            initial_prompt,
            channel,
        ) {
            push_segment(seg, all, shared);
        }
    }
}

/// Check whether STT output is a non-speech artifact (silence markers,
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

/// Run STT inference on a 16 kHz mono audio segment, send the result
/// to the frontend, and return a clone for accumulation.
fn transcribe_segment(
    backend: &mut dyn SttBackend,
    audio: &[f32],
    speaker: Speaker,
    start_ms: u64,
    end_ms: u64,
    initial_prompt: Option<&str>,
    channel: &Channel<TranscriptSegment>,
) -> Option<TranscriptSegment> {
    let text = match backend.transcribe(audio, initial_prompt) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Transcription failed: {}", e);
            return None;
        }
    };

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
}
