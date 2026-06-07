//! Voice activity detection (VAD) and speech-driven segmentation.
//!
//! The transcription worker doesn't just need a yes/no "is this loud" check —
//! it needs to slice the live audio stream into tight segments around real
//! speech so the STT engine never sees long silence-padded buffers (which make
//! models like Parakeet return empty and drop short utterances).
//!
//! This module provides:
//! - A [`Vad`] trait: a streaming detector that returns a speech *probability*
//!   per fixed-size frame of 16 kHz mono audio.
//! - Three backends behind that trait — [`SileroVad`] (default, neural),
//!   [`TenVad`] (neural), and [`EnergyVad`] (RMS fallback) — selected via
//!   [`VadKind`].
//! - A [`Segmenter`] state machine that consumes frames, tracks speech
//!   onset/offset with pre-roll padding and a silence hangover, and emits
//!   finalized speech segments ready for transcription.

mod energy;
mod segmenter;
mod silero;
mod ten;

pub use energy::EnergyVad;
pub use segmenter::{Segmenter, SegmenterConfig};
pub use silero::SileroVad;
pub use ten::TenVad;

use serde::{Deserialize, Serialize};
use std::fmt;

/// Sample rate (Hz) that every VAD backend and the segmenter operate at.
pub const VAD_SAMPLE_RATE: u32 = 16_000;

/// A streaming voice activity detector over 16 kHz mono audio.
///
/// Implementations are stateful (neural backends carry recurrent state across
/// calls), so each audio stream needs its own instance.
pub trait Vad: Send {
    /// Number of 16 kHz mono samples consumed per [`predict`](Vad::predict) call.
    fn frame_size(&self) -> usize;

    /// Return the speech probability in `[0.0, 1.0]` for one frame.
    ///
    /// The frame should be exactly [`frame_size`](Vad::frame_size) samples;
    /// shorter frames are zero-padded by the implementation.
    fn predict(&mut self, frame: &[f32]) -> f32;

    /// Clear streaming state between recordings. No-op for stateless backends.
    fn reset(&mut self);
}

/// Identifies which VAD backend to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VadKind {
    /// Silero VAD v5 (neural). The default — only independently-validated model.
    #[default]
    Silero,
    /// TEN VAD (neural).
    Ten,
    /// RMS energy threshold (legacy fallback).
    Energy,
}

impl fmt::Display for VadKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VadKind::Silero => write!(f, "silero"),
            VadKind::Ten => write!(f, "ten"),
            VadKind::Energy => write!(f, "energy"),
        }
    }
}

/// Parse a string into a [`VadKind`], defaulting to Silero for unknown values.
pub fn parse_vad_kind(s: &str) -> VadKind {
    match s.to_lowercase().as_str() {
        "ten" => VadKind::Ten,
        "energy" => VadKind::Energy,
        _ => VadKind::Silero,
    }
}

/// Create the VAD backend for the given kind, loading any bundled model.
///
/// `energy_threshold` is only used by [`VadKind::Energy`]; the neural backends
/// score with their own learned model and are gated by the segmenter threshold.
pub fn create_vad(kind: VadKind, energy_threshold: f32) -> Result<Box<dyn Vad>, String> {
    match kind {
        VadKind::Silero => Ok(Box::new(SileroVad::load()?)),
        VadKind::Ten => Ok(Box::new(TenVad::load()?)),
        VadKind::Energy => Ok(Box::new(EnergyVad::new(energy_threshold))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_vad_kind_defaults_to_silero() {
        // Arrange / Act / Assert
        assert_eq!(parse_vad_kind("nonsense"), VadKind::Silero);
        assert_eq!(parse_vad_kind(""), VadKind::Silero);
    }

    #[test]
    fn parse_vad_kind_recognizes_known_values() {
        // Arrange / Act / Assert
        assert_eq!(parse_vad_kind("silero"), VadKind::Silero);
        assert_eq!(parse_vad_kind("TEN"), VadKind::Ten);
        assert_eq!(parse_vad_kind("Energy"), VadKind::Energy);
    }

    #[test]
    fn vad_kind_display_roundtrips_through_parse() {
        // Arrange
        let kinds = [VadKind::Silero, VadKind::Ten, VadKind::Energy];

        for kind in kinds {
            // Act
            let parsed = parse_vad_kind(&kind.to_string());

            // Assert
            assert_eq!(parsed, kind);
        }
    }

    #[test]
    fn vad_kind_default_is_silero() {
        // Arrange / Act / Assert
        assert_eq!(VadKind::default(), VadKind::Silero);
    }

    #[test]
    fn create_vad_energy_reports_expected_frame_size() {
        // Arrange / Act — energy needs no model load, so it's safe in unit tests
        let vad = create_vad(VadKind::Energy, 0.005).expect("energy VAD should load");

        // Assert
        assert_eq!(vad.frame_size(), energy::ENERGY_FRAME_SIZE);
    }
}
