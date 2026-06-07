//! RMS energy VAD — the legacy fallback backend.
//!
//! Reuses the existing energy detector from [`crate::audio::pipeline`]. It only
//! reacts to loudness, so it can't distinguish a quiet word from background
//! noise; the neural backends ([`super::SileroVad`], [`super::TenVad`]) are
//! preferred. Kept selectable for environments where the neural models can't
//! load or for comparison.

use super::Vad;
use crate::audio::pipeline;

/// Frame size (32 ms at 16 kHz) the energy VAD scores at, matching Silero so
/// the segmenter behaves consistently across backends.
pub(super) const ENERGY_FRAME_SIZE: usize = 512;

/// Energy-threshold voice activity detector.
pub struct EnergyVad {
    /// RMS threshold above which a frame is considered speech.
    threshold: f32,
}

impl EnergyVad {
    /// Create an energy VAD with the given RMS threshold.
    pub fn new(threshold: f32) -> Self {
        Self { threshold }
    }
}

impl Vad for EnergyVad {
    fn frame_size(&self) -> usize {
        ENERGY_FRAME_SIZE
    }

    /// Returns `1.0` if the frame's RMS exceeds the threshold, else `0.0`.
    /// The hard 0/1 mapping pairs with the segmenter's `0.5` default threshold.
    fn predict(&mut self, frame: &[f32]) -> f32 {
        if pipeline::is_speech(frame, self.threshold) {
            1.0
        } else {
            0.0
        }
    }

    fn reset(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn predict_returns_one_for_loud_frame() {
        // Arrange — constant 0.5 amplitude is well above the threshold
        let mut vad = EnergyVad::new(0.01);
        let frame = vec![0.5; ENERGY_FRAME_SIZE];

        // Act
        let prob = vad.predict(&frame);

        // Assert
        assert_eq!(prob, 1.0);
    }

    #[test]
    fn predict_returns_zero_for_silence() {
        // Arrange
        let mut vad = EnergyVad::new(0.01);
        let frame = vec![0.0; ENERGY_FRAME_SIZE];

        // Act
        let prob = vad.predict(&frame);

        // Assert
        assert_eq!(prob, 0.0);
    }

    #[test]
    fn frame_size_is_stable() {
        // Arrange / Act / Assert
        let vad = EnergyVad::new(0.005);
        assert_eq!(vad.frame_size(), ENERGY_FRAME_SIZE);
    }
}
