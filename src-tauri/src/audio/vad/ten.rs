//! TEN VAD backend (neural), via the `ten-vad-rs` crate.
//!
//! TEN VAD is a lightweight enterprise VAD. The crate manages the model's five
//! recurrent inputs internally, so we just feed 16 ms (256-sample) frames of
//! 16 kHz PCM and read back a speech score. The bundled model is loaded from
//! memory to avoid a temp file.

use ten_vad_rs::TenVad as TenVadModel;

use super::Vad;

/// The TEN VAD model, embedded in the binary.
static TEN_MODEL: &[u8] = include_bytes!("../../../resources/vad/ten-vad.onnx");

/// TEN's reference hop size (256 samples = 16 ms at 16 kHz).
pub(super) const TEN_FRAME_SIZE: usize = 256;

/// Full-scale value for converting normalized f32 audio to 16-bit PCM.
const I16_SCALE: f32 = 32767.0;

/// TEN VAD neural backend.
pub struct TenVad {
    /// The loaded TEN VAD model runner.
    model: TenVadModel,
}

impl TenVad {
    /// Load the bundled TEN VAD model from memory.
    pub fn load() -> Result<Self, String> {
        let model = TenVadModel::new_from_bytes(TEN_MODEL, super::VAD_SAMPLE_RATE)
            .map_err(|e| format!("Failed to load TEN VAD model: {e}"))?;
        Ok(Self { model })
    }
}

impl Vad for TenVad {
    fn frame_size(&self) -> usize {
        TEN_FRAME_SIZE
    }

    fn predict(&mut self, frame: &[f32]) -> f32 {
        // TEN consumes raw i16 PCM. Convert from our normalized [-1, 1] f32 and
        // zero-pad a short final frame to the expected hop length.
        let mut pcm: Vec<i16> = frame
            .iter()
            .map(|&s| (s.clamp(-1.0, 1.0) * I16_SCALE) as i16)
            .collect();
        pcm.resize(TEN_FRAME_SIZE, 0);

        // A failed frame is treated as non-speech rather than crashing the worker.
        self.model.process_frame(&pcm).unwrap_or(0.0)
    }

    fn reset(&mut self) {
        self.model.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_bundled_model_and_reports_frame_size() {
        // Arrange / Act
        let vad = TenVad::load().expect("bundled TEN model should load");

        // Assert
        assert_eq!(vad.frame_size(), TEN_FRAME_SIZE);
    }

    #[test]
    fn predicts_low_probability_on_silence() {
        // Arrange
        let mut vad = TenVad::load().expect("bundled TEN model should load");
        let silence = vec![0.0; TEN_FRAME_SIZE];

        // Act — feed several frames so the sliding feature window settles
        let mut prob = 1.0;
        for _ in 0..10 {
            prob = vad.predict(&silence);
        }

        // Assert
        assert!(prob < 0.5, "silence scored {prob}, expected < 0.5");
    }
}
