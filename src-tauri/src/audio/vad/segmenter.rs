//! Speech-onset/offset segmentation driven by a [`Vad`].
//!
//! Replaces the old "accumulate everything, then RMS-check the tail" approach.
//! The segmenter feeds fixed-size frames to a [`Vad`], tracks a small state
//! machine, and emits tight speech segments:
//!
//! - A **pre-roll ring** keeps the most recent audio so a segment includes the
//!   moment *before* speech was confirmed — this captures the attack of short
//!   words like "yes".
//! - An **onset** requires a brief run of speech frames (`min_speech_ms`) to
//!   avoid triggering on single-frame blips.
//! - An **offset** waits out a silence **hangover** (`min_silence_ms`) so brief
//!   pauses within a sentence don't split it.
//! - Segments are force-cut at `max_segment_ms` to bound latency.
//! - Short segments are zero-padded to `min_emit_ms` so models like Parakeet,
//!   which drop sub-second clips, still transcribe them.

use std::collections::VecDeque;

use super::{Vad, VAD_SAMPLE_RATE};

/// Convert milliseconds to a count of 16 kHz samples.
fn ms_to_samples(ms: u32) -> usize {
    (ms as usize) * (VAD_SAMPLE_RATE as usize) / 1000
}

/// Tuning for the [`Segmenter`]. All durations are in milliseconds.
#[derive(Debug, Clone, Copy)]
pub struct SegmenterConfig {
    /// Speech probability at/above which a frame counts as speech.
    pub threshold: f32,
    /// Sustained speech required to confirm onset (kept low to catch short words).
    pub min_speech_ms: u32,
    /// Silence hangover after speech before a segment is cut.
    pub min_silence_ms: u32,
    /// Pre-roll kept before onset so a segment includes the word's attack.
    pub speech_pad_ms: u32,
    /// Hard cap on segment length before a forced cut.
    pub max_segment_ms: u32,
    /// Segments shorter than this are zero-padded up to it before emission.
    pub min_emit_ms: u32,
}

impl Default for SegmenterConfig {
    fn default() -> Self {
        Self {
            // Tuned for high recall — meetings must capture brief, quiet replies
            // ("wow", "why not"). Silence scores ~0.04, so a 0.3 gate stays well
            // clear of the noise floor while catching soft/short words, and a
            // single voiced frame (32 ms) is enough to confirm onset.
            threshold: 0.3,
            min_speech_ms: 32,
            min_silence_ms: 384,
            speech_pad_ms: 300,
            max_segment_ms: 15_000,
            min_emit_ms: 1_000,
        }
    }
}

/// Whether the segmenter is currently inside a speech run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Silence,
    Speech,
}

/// Turns a stream of 16 kHz mono samples into finalized speech segments.
pub struct Segmenter {
    /// The detector scoring each frame.
    vad: Box<dyn Vad>,
    /// Samples per VAD frame (cached from `vad`).
    frame_size: usize,
    /// Pre-computed thresholds in samples.
    min_speech_samples: usize,
    min_silence_samples: usize,
    pad_samples: usize,
    max_segment_samples: usize,
    min_emit_samples: usize,
    threshold: f32,

    /// Leftover samples not yet forming a full frame.
    feed: Vec<f32>,
    /// Rolling window of the most recent `pad_samples` samples.
    pre_roll: VecDeque<f32>,
    /// Audio accumulated for the in-progress segment.
    current: Vec<f32>,
    /// Current onset/offset state.
    state: State,
    /// Consecutive speech samples seen while in `Silence` (onset counter).
    speech_run: usize,
    /// Consecutive silence samples seen while in `Speech` (offset counter).
    silence_run: usize,
}

impl Segmenter {
    /// Create a segmenter wrapping the given VAD with the given tuning.
    pub fn new(vad: Box<dyn Vad>, config: SegmenterConfig) -> Self {
        let frame_size = vad.frame_size().max(1);
        Self {
            vad,
            frame_size,
            min_speech_samples: ms_to_samples(config.min_speech_ms),
            min_silence_samples: ms_to_samples(config.min_silence_ms),
            pad_samples: ms_to_samples(config.speech_pad_ms),
            max_segment_samples: ms_to_samples(config.max_segment_ms),
            min_emit_samples: ms_to_samples(config.min_emit_ms),
            threshold: config.threshold,
            feed: Vec::new(),
            pre_roll: VecDeque::new(),
            current: Vec::new(),
            state: State::Silence,
            speech_run: 0,
            silence_run: 0,
        }
    }

    /// Feed more 16 kHz mono samples; returns any segments finalized this call.
    pub fn push_samples(&mut self, samples: &[f32]) -> Vec<Vec<f32>> {
        self.feed.extend_from_slice(samples);

        let mut out = Vec::new();
        while self.feed.len() >= self.frame_size {
            let frame: Vec<f32> = self.feed.drain(..self.frame_size).collect();
            self.process_frame(&frame, &mut out);
        }
        out
    }

    /// Emit any in-progress speech segment at end of stream.
    pub fn flush(&mut self) -> Option<Vec<f32>> {
        if self.state == State::Speech && !self.current.is_empty() {
            let seg = std::mem::take(&mut self.current);
            self.reset_run_state();
            return Some(self.pad_to_min(seg));
        }
        None
    }

    /// Clear all state for reuse (also resets the underlying VAD).
    #[allow(dead_code)]
    pub fn reset(&mut self) {
        self.vad.reset();
        self.feed.clear();
        self.pre_roll.clear();
        self.current.clear();
        self.reset_run_state();
    }

    /// Process exactly one frame, pushing any finalized segment into `out`.
    fn process_frame(&mut self, frame: &[f32], out: &mut Vec<Vec<f32>>) {
        let prob = self.vad.predict(frame);
        let is_speech = prob >= self.threshold;

        // The pre-roll always tracks the most recent audio, including this frame,
        // so an onset can reach back to capture the word's attack.
        self.push_pre_roll(frame);

        match self.state {
            State::Silence => {
                if is_speech {
                    self.speech_run += frame.len();
                    if self.speech_run >= self.min_speech_samples {
                        // Onset confirmed: seed the segment with the pre-roll
                        // (which already includes this frame).
                        self.state = State::Speech;
                        self.current = self.pre_roll.iter().copied().collect();
                        self.silence_run = 0;
                    }
                } else {
                    self.speech_run = 0;
                }
            }
            State::Speech => {
                self.current.extend_from_slice(frame);
                if is_speech {
                    self.silence_run = 0;
                } else {
                    self.silence_run += frame.len();
                    if self.silence_run >= self.min_silence_samples {
                        if let Some(seg) = self.finalize() {
                            out.push(seg);
                        }
                        return;
                    }
                }

                // Bound latency on long, unbroken speech.
                if self.current.len() >= self.max_segment_samples {
                    if let Some(seg) = self.force_cut() {
                        out.push(seg);
                    }
                }
            }
        }
    }

    /// Finalize a segment on a normal speech→silence offset, returning to silence.
    fn finalize(&mut self) -> Option<Vec<f32>> {
        let seg = std::mem::take(&mut self.current);
        self.state = State::Silence;
        self.reset_run_state();
        if seg.is_empty() {
            None
        } else {
            Some(self.pad_to_min(seg))
        }
    }

    /// Emit the current segment but stay in speech, seeding the next segment
    /// with the pre-roll so a word straddling the cut isn't lost.
    fn force_cut(&mut self) -> Option<Vec<f32>> {
        let seg = std::mem::take(&mut self.current);
        self.current = self.pre_roll.iter().copied().collect();
        if seg.is_empty() {
            None
        } else {
            Some(self.pad_to_min(seg))
        }
    }

    /// Append a frame to the bounded pre-roll ring.
    fn push_pre_roll(&mut self, frame: &[f32]) {
        if self.pad_samples == 0 {
            return;
        }
        for &s in frame {
            if self.pre_roll.len() >= self.pad_samples {
                self.pre_roll.pop_front();
            }
            self.pre_roll.push_back(s);
        }
    }

    /// Zero-pad a segment up to `min_emit_samples` so short clips aren't dropped
    /// by the STT engine.
    fn pad_to_min(&self, mut seg: Vec<f32>) -> Vec<f32> {
        if seg.len() < self.min_emit_samples {
            seg.resize(self.min_emit_samples, 0.0);
        }
        seg
    }

    /// Reset the onset/offset counters and silence state.
    fn reset_run_state(&mut self) {
        self.state = State::Silence;
        self.speech_run = 0;
        self.silence_run = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    /// A VAD whose output is scripted, so the segmenter can be tested with no
    /// real audio or model. Returns queued probabilities, then `0.0`.
    struct FakeVad {
        frame_size: usize,
        probs: VecDeque<f32>,
    }

    impl FakeVad {
        fn new(frame_size: usize, probs: Vec<f32>) -> Self {
            Self {
                frame_size,
                probs: probs.into(),
            }
        }
    }

    impl Vad for FakeVad {
        fn frame_size(&self) -> usize {
            self.frame_size
        }
        fn predict(&mut self, _frame: &[f32]) -> f32 {
            self.probs.pop_front().unwrap_or(0.0)
        }
        fn reset(&mut self) {
            self.probs.clear();
        }
    }

    /// Frame size of 16 samples = 1 ms at 16 kHz, so config ms map cleanly:
    /// 1 ms == 1 frame == 16 samples.
    const FRAME: usize = 16;

    /// Build a config sized for the 1 ms/frame test scale.
    fn test_config(min_speech_ms: u32, min_silence_ms: u32, pad_ms: u32) -> SegmenterConfig {
        SegmenterConfig {
            threshold: 0.5,
            min_speech_ms,
            min_silence_ms,
            speech_pad_ms: pad_ms,
            max_segment_ms: 1_000,
            min_emit_ms: 0, // disable padding so segment lengths are exact
        }
    }

    /// `n` frames worth of dummy samples (content is irrelevant to FakeVad).
    fn frames(n: usize) -> Vec<f32> {
        vec![0.1; n * FRAME]
    }

    #[test]
    fn short_burst_emits_one_segment_including_preroll() {
        // Arrange — 3 silence, 2 speech (onset at 2nd), 4 silence (offset)
        let probs = vec![0.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0];
        let vad = FakeVad::new(FRAME, probs);
        let mut seg = Segmenter::new(Box::new(vad), test_config(2, 4, 3));

        // Act
        let out = seg.push_samples(&frames(9));

        // Assert — pre-roll (3 frames) + 4 appended silence frames = 7 frames
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].len(), 7 * FRAME);
    }

    #[test]
    fn brief_pause_does_not_cut_segment() {
        // Arrange — onset, a 3-frame pause (< 4 ms hangover), speech, then offset
        let probs = vec![1.0, 1.0, 0.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0];
        let vad = FakeVad::new(FRAME, probs);
        let mut seg = Segmenter::new(Box::new(vad), test_config(2, 4, 3));

        // Act
        let out = seg.push_samples(&frames(11));

        // Assert — the brief pause is bridged; exactly one segment is emitted
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn all_silence_emits_nothing() {
        // Arrange
        let vad = FakeVad::new(FRAME, vec![0.0; 20]);
        let mut seg = Segmenter::new(Box::new(vad), test_config(2, 4, 3));

        // Act
        let out = seg.push_samples(&frames(20));

        // Assert
        assert!(out.is_empty());
        assert!(seg.flush().is_none());
    }

    #[test]
    fn flush_emits_trailing_speech_without_offset() {
        // Arrange — onset then recording ends mid-speech (no trailing silence)
        let probs = vec![1.0, 1.0, 1.0, 1.0];
        let vad = FakeVad::new(FRAME, probs);
        let mut seg = Segmenter::new(Box::new(vad), test_config(2, 4, 3));

        // Act
        let during = seg.push_samples(&frames(4));
        let flushed = seg.flush();

        // Assert — nothing cut mid-stream, but flush recovers the segment
        assert!(during.is_empty());
        assert!(flushed.is_some());
    }

    #[test]
    fn max_segment_forces_a_cut_during_long_speech() {
        // Arrange — continuous speech longer than max_segment_ms
        let mut config = test_config(2, 100, 3);
        config.max_segment_ms = 5; // 5 frames
        let vad = FakeVad::new(FRAME, vec![1.0; 40]);
        let mut seg = Segmenter::new(Box::new(vad), config);

        // Act
        let out = seg.push_samples(&frames(40));

        // Assert — at least one forced cut happened mid-stream
        assert!(!out.is_empty());
    }

    #[test]
    fn pad_to_min_pads_short_segments() {
        // Arrange — short burst, but require a 50 ms minimum emit length
        let mut config = test_config(2, 4, 3);
        config.min_emit_ms = 50; // 50 frames = 800 samples
        let probs = vec![1.0, 1.0, 0.0, 0.0, 0.0, 0.0];
        let vad = FakeVad::new(FRAME, probs);
        let mut seg = Segmenter::new(Box::new(vad), config);

        // Act
        let out = seg.push_samples(&frames(6));

        // Assert — emitted segment is padded up to the minimum length
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].len(), 50 * FRAME);
    }

    #[test]
    fn single_frame_blip_does_not_confirm_onset() {
        // Arrange — one speech frame surrounded by silence, min_speech = 2 frames
        let probs = vec![0.0, 1.0, 0.0, 0.0, 0.0, 0.0];
        let vad = FakeVad::new(FRAME, probs);
        let mut seg = Segmenter::new(Box::new(vad), test_config(2, 4, 3));

        // Act
        let out = seg.push_samples(&frames(6));

        // Assert — a lone blip is below the onset threshold, nothing emitted
        assert!(out.is_empty());
        assert!(seg.flush().is_none());
    }

    #[test]
    fn samples_are_buffered_across_calls() {
        // Arrange — feed half-frames so a frame spans two push_samples calls
        let probs = vec![1.0, 1.0, 0.0, 0.0, 0.0, 0.0];
        let vad = FakeVad::new(FRAME, probs);
        let mut seg = Segmenter::new(Box::new(vad), test_config(2, 4, 3));

        // Act — 6 frames worth of samples, delivered 8 samples at a time
        let all = frames(6);
        let mut out = Vec::new();
        for chunk in all.chunks(8) {
            out.extend(seg.push_samples(chunk));
        }

        // Assert — partial chunks are reassembled into whole frames and a
        // segment is still produced
        assert_eq!(out.len(), 1);
    }
}
