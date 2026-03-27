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

/// Number of 16 kHz samples for segment overlap (~300ms).
/// Prepended to the next segment to prevent word truncation at boundaries.
const OVERLAP_SAMPLES: usize = 4_800;

/// Configuration for starting a transcription worker.
///
/// Bundles all the parameters needed to spawn the background transcription
/// thread, keeping function signatures manageable.
pub struct WorkerConfig {
    /// Path to the ggml Whisper model file.
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
    /// Shared VAD threshold value.
    pub vad_threshold: Arc<AtomicU32>,
    /// Auto-stop after this many seconds of silence.
    pub silence_timeout_seconds: Option<u32>,
    /// Optional prompt to guide Whisper (domain terms, names).
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
    config: WorkerConfig,
) -> Vec<TranscriptSegment> {
    let WorkerConfig {
        model_path,
        mut mic_consumer,
        mut system_consumer,
        mic_sample_rate,
        mic_channels,
        system_sample_rate,
        system_channels,
        vad_threshold,
        silence_timeout_seconds,
        initial_prompt,
        max_segment_seconds,
        channel,
    } = config;

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

    // Compute max segment size in 16 kHz samples from the configured seconds
    let segment_samples = (max_segment_seconds.clamp(1, 30) as usize) * 16_000;

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

    // Overlap buffers: tail of the previous 16 kHz mono segment, prepended
    // to the next segment so Whisper has context across boundaries and
    // doesn't truncate words at the cut point.
    let mut mic_overlap: Vec<f32> = Vec::new();
    let mut system_overlap: Vec<f32> = Vec::new();

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
        if should_transcribe(
            &mic_mono,
            &mut mic_had_speech,
            &mut mic_silence_count,
            threshold,
            segment_samples,
        ) {
            let elapsed_ms = recording_start.elapsed().as_millis() as u64;
            if mic_had_speech {
                // Prepend overlap from previous segment for word-boundary context
                let audio = prepend_overlap(&mic_overlap, &mic_mono);
                if let Some(seg) = transcribe_segment(
                    &ctx,
                    &audio,
                    Speaker::Me,
                    mic_segment_start_ms,
                    elapsed_ms,
                    initial_prompt.as_deref(),
                    &channel,
                ) {
                    push_segment(seg, &mut all_segments, &shared_segments);
                }
            }
            // Save tail as overlap for next segment
            mic_overlap = tail_overlap(&mic_mono);
            mic_raw_buf.clear();
            mic_segment_start_ms = elapsed_ms;
            mic_had_speech = false;
            mic_silence_count = 0;
        }

        // Process system segment
        let system_mono =
            pipeline::process_buffer(&system_raw_buf, system_sample_rate, system_channels);
        if should_transcribe(
            &system_mono,
            &mut system_had_speech,
            &mut system_silence_count,
            threshold,
            segment_samples,
        ) {
            let elapsed_ms = recording_start.elapsed().as_millis() as u64;
            if system_had_speech {
                let audio = prepend_overlap(&system_overlap, &system_mono);
                if let Some(seg) = transcribe_segment(
                    &ctx,
                    &audio,
                    Speaker::Others,
                    system_segment_start_ms,
                    elapsed_ms,
                    initial_prompt.as_deref(),
                    &channel,
                ) {
                    push_segment(seg, &mut all_segments, &shared_segments);
                }
            }
            system_overlap = tail_overlap(&system_mono);
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
            if timeout_secs > 0 && last_speech_time.elapsed().as_secs() >= u64::from(timeout_secs) {
                eprintln!("Auto-stopping: no speech for {} seconds", timeout_secs);
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
        let audio = prepend_overlap(&mic_overlap, &mic_mono);
        if let Some(seg) = transcribe_segment(
            &ctx,
            &audio,
            Speaker::Me,
            mic_segment_start_ms,
            elapsed_ms,
            initial_prompt.as_deref(),
            &channel,
        ) {
            push_segment(seg, &mut all_segments, &shared_segments);
        }
    }

    let system_mono =
        pipeline::process_buffer(&system_raw_buf, system_sample_rate, system_channels);
    if !system_mono.is_empty() && pipeline::is_speech(&system_mono, threshold) {
        let audio = prepend_overlap(&system_overlap, &system_mono);
        if let Some(seg) = transcribe_segment(
            &ctx,
            &audio,
            Speaker::Others,
            system_segment_start_ms,
            elapsed_ms,
            initial_prompt.as_deref(),
            &channel,
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

/// Return the last `OVERLAP_SAMPLES` of a 16 kHz mono buffer for use as
/// overlap context in the next segment.
fn tail_overlap(mono_16k: &[f32]) -> Vec<f32> {
    let start = mono_16k.len().saturating_sub(OVERLAP_SAMPLES);
    mono_16k[start..].to_vec()
}

/// Prepend overlap samples from the previous segment to the current one.
/// Returns a new buffer with the overlap context followed by the current audio.
fn prepend_overlap(overlap: &[f32], current: &[f32]) -> Vec<f32> {
    if overlap.is_empty() {
        return current.to_vec();
    }
    let mut combined = Vec::with_capacity(overlap.len() + current.len());
    combined.extend_from_slice(overlap);
    combined.extend_from_slice(current);
    combined
}

/// Decide whether the accumulated 16 kHz mono buffer is ready to transcribe.
///
/// Uses a trailing window for VAD so short utterances are detected even
/// when surrounded by silence. A silence holdoff counter prevents cutting
/// segments on brief pauses between words.
///
/// Triggers on:
/// 1. Buffer reaching the max segment length.
/// 2. Sustained silence after speech (holdoff expired, minimum buffer met).
fn should_transcribe(
    mono_16k: &[f32],
    had_speech: &mut bool,
    silence_count: &mut u32,
    threshold: f32,
    segment_samples: usize,
) -> bool {
    if mono_16k.len() >= segment_samples {
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
    initial_prompt: Option<&str>,
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

    if let Some(prompt) = initial_prompt {
        params.set_initial_prompt(prompt);
    }

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

    /// Default segment size used in tests (15 seconds at 16 kHz).
    const TEST_SEGMENT_SAMPLES: usize = 240_000;

    #[test]
    fn should_transcribe_triggers_on_max_segment_length() {
        // Arrange — buffer at exactly the configured segment size
        let mono: Vec<f32> = vec![0.0; TEST_SEGMENT_SAMPLES];
        let mut had_speech = false;
        let mut silence_count = 0u32;
        let threshold = 0.01;

        // Act
        let result = should_transcribe(
            &mono,
            &mut had_speech,
            &mut silence_count,
            threshold,
            TEST_SEGMENT_SAMPLES,
        );

        // Assert
        assert!(result);
    }

    #[test]
    fn should_transcribe_respects_custom_segment_size() {
        // Arrange — use a smaller segment size (5 seconds)
        let custom_segment = 80_000;
        let mono: Vec<f32> = vec![0.0; custom_segment];
        let mut had_speech = false;
        let mut silence_count = 0u32;
        let threshold = 0.01;

        // Act
        let result = should_transcribe(
            &mono,
            &mut had_speech,
            &mut silence_count,
            threshold,
            custom_segment,
        );

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
        let result = should_transcribe(
            &mono,
            &mut had_speech,
            &mut silence_count,
            threshold,
            TEST_SEGMENT_SAMPLES,
        );

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
        let result = should_transcribe(
            &mono,
            &mut had_speech,
            &mut silence_count,
            threshold,
            TEST_SEGMENT_SAMPLES,
        );

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
        let result = should_transcribe(
            &mono,
            &mut had_speech,
            &mut silence_count,
            threshold,
            TEST_SEGMENT_SAMPLES,
        );

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
        let _result = should_transcribe(
            &mono,
            &mut had_speech,
            &mut silence_count,
            threshold,
            TEST_SEGMENT_SAMPLES,
        );

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
        let result = should_transcribe(
            &mono,
            &mut had_speech,
            &mut silence_count,
            threshold,
            TEST_SEGMENT_SAMPLES,
        );

        // Assert — holdoff reached but buffer too short
        assert!(!result);
    }

    // ── tail_overlap ────────────────────────────

    #[test]
    fn tail_overlap_returns_last_n_samples() {
        // Arrange
        let audio: Vec<f32> = (0..10_000).map(|i| i as f32).collect();

        // Act
        let overlap = tail_overlap(&audio);

        // Assert
        assert_eq!(overlap.len(), OVERLAP_SAMPLES);
        assert_eq!(overlap[0], (10_000 - OVERLAP_SAMPLES) as f32);
        assert_eq!(*overlap.last().unwrap(), 9_999.0);
    }

    #[test]
    fn tail_overlap_returns_all_when_shorter_than_overlap() {
        // Arrange
        let audio: Vec<f32> = vec![1.0, 2.0, 3.0];

        // Act
        let overlap = tail_overlap(&audio);

        // Assert
        assert_eq!(overlap, vec![1.0, 2.0, 3.0]);
    }

    // ── prepend_overlap ────────────────────────────

    #[test]
    fn prepend_overlap_combines_buffers() {
        // Arrange
        let overlap = vec![1.0, 2.0, 3.0];
        let current = vec![4.0, 5.0, 6.0];

        // Act
        let combined = prepend_overlap(&overlap, &current);

        // Assert
        assert_eq!(combined, vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
    }

    #[test]
    fn prepend_overlap_returns_current_when_overlap_empty() {
        // Arrange
        let overlap: Vec<f32> = vec![];
        let current = vec![4.0, 5.0, 6.0];

        // Act
        let combined = prepend_overlap(&overlap, &current);

        // Assert
        assert_eq!(combined, vec![4.0, 5.0, 6.0]);
    }
}
