//! Silero VAD v5 backend (neural), run directly on ONNX Runtime via `ort`.
//!
//! We run the bundled ~2.2 MB model on the `ort` already pulled in by
//! `transcribe-rs` (same version, so only one ONNX Runtime is linked). The
//! model is recurrent: each call takes the previous `state` tensor and returns
//! a new one, which we carry across frames for a single audio stream.
//!
//! Verified model I/O (Silero v5):
//! - inputs:  `input` f32 `[batch, samples]`, `state` f32 `[2, batch, 128]`,
//!   `sr` i64 scalar
//! - outputs: `output` f32 `[batch, 1]` (speech prob), `stateN` f32 (new state)

use ndarray::{Array2, Array3, ArrayD};
use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use ort::value::Tensor;

use super::{Vad, VAD_SAMPLE_RATE};

/// The Silero v5 model, embedded in the binary.
static SILERO_MODEL: &[u8] = include_bytes!("../../../resources/vad/silero_vad.onnx");

/// Required window size for Silero v5 at 16 kHz (512 samples = 32 ms).
pub(super) const SILERO_FRAME_SIZE: usize = 512;

/// Flattened length of the recurrent state tensor (`2 * 1 * 128`).
const STATE_LEN: usize = 2 * 128;

/// Silero VAD v5 neural backend.
pub struct SileroVad {
    /// The loaded ONNX Runtime session.
    session: Session,
    /// Recurrent hidden state carried between frames (`[2, 1, 128]`).
    state: ArrayD<f32>,
}

impl SileroVad {
    /// Load the bundled Silero model into an ONNX Runtime session.
    ///
    /// A single intra-op thread keeps per-frame latency low and avoids
    /// contending with the STT engine's own thread pool.
    pub fn load() -> Result<Self, String> {
        Self::from_bytes(SILERO_MODEL)
    }

    /// Build a Silero session from raw ONNX model bytes.
    fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        let session = Session::builder()
            .map_err(|e| format!("Failed to create Silero session builder: {e}"))?
            .with_intra_threads(1)
            .map_err(|e| format!("Failed to configure Silero threads: {e}"))?
            .with_optimization_level(GraphOptimizationLevel::Disable)
            .map_err(|e| format!("Failed to set Silero optimization level: {e}"))?
            .commit_from_memory(bytes)
            .map_err(|e| format!("Failed to load Silero model: {e}"))?;

        Ok(Self {
            session,
            state: Array3::<f32>::zeros((2, 1, 128)).into_dyn(),
        })
    }
}

impl Vad for SileroVad {
    fn frame_size(&self) -> usize {
        SILERO_FRAME_SIZE
    }

    fn predict(&mut self, frame: &[f32]) -> f32 {
        // Silero requires exactly 512 samples; zero-pad a short final frame.
        let mut samples = frame.to_vec();
        samples.resize(SILERO_FRAME_SIZE, 0.0);

        // Build inputs as owned ndarray tensors, mirroring the reference Silero
        // bindings exactly: input `[1, 512]`, state `[2, 1, 128]`, and `sr` as a
        // 0-D (scalar) int64. Inputs are passed positionally in the model's
        // declared order (input, state, sr).
        let input = match Array2::from_shape_vec((1, SILERO_FRAME_SIZE), samples) {
            Ok(a) => a,
            Err(_) => return 0.0,
        };
        let sr = ndarray::arr0::<i64>(i64::from(VAD_SAMPLE_RATE));

        // On any tensor/inference error we return 0.0 (treat as non-speech)
        // rather than panicking the worker thread — a dropped frame is far
        // better than a crashed recording.
        let input_t = match Tensor::from_array(input) {
            Ok(t) => t,
            Err(_) => return 0.0,
        };
        let state_t = match Tensor::from_array(self.state.clone()) {
            Ok(t) => t,
            Err(_) => return 0.0,
        };
        let sr_t = match Tensor::from_array(sr) {
            Ok(t) => t,
            Err(_) => return 0.0,
        };

        let outputs = match self.session.run(ort::inputs![input_t, state_t, sr_t]) {
            Ok(o) => o,
            Err(e) => {
                eprintln!("[vad] silero inference error: {e}");
                return 0.0;
            }
        };

        // Extract the probability and the next state before dropping `outputs`
        // (which borrows the session).
        let prob = outputs
            .get("output")
            .and_then(|v| v.try_extract_array::<f32>().ok())
            .and_then(|a| a.iter().next().copied())
            .unwrap_or(0.0);

        let next_state = outputs
            .get("stateN")
            .and_then(|v| v.try_extract_array::<f32>().ok())
            .map(|a| a.to_owned());

        drop(outputs);

        if let Some(new_state) = next_state {
            if new_state.len() == STATE_LEN {
                self.state = new_state;
            }
        }

        prob
    }

    fn reset(&mut self) {
        self.state.fill(0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_bundled_model_and_reports_frame_size() {
        // Arrange / Act
        let vad = SileroVad::load().expect("bundled Silero model should load");

        // Assert
        assert_eq!(vad.frame_size(), SILERO_FRAME_SIZE);
    }

    #[test]
    fn predicts_low_probability_on_silence() {
        // Arrange — a full frame of silence
        let mut vad = SileroVad::load().expect("bundled Silero model should load");
        let silence = vec![0.0; SILERO_FRAME_SIZE];

        // Act
        let prob = vad.predict(&silence);

        // Assert — silence should score well below the speech threshold
        assert!(prob < 0.5, "silence scored {prob}, expected < 0.5");
    }

    /// Read the bundled 16 kHz mono i16 WAV fixture into f32 samples in [-1, 1].
    fn read_fixture_speech() -> Vec<f32> {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/resources/test/jfk.wav");
        let bytes = std::fs::read(path).expect("speech fixture should exist");
        // Skip the 44-byte canonical WAV header, decode i16 LE PCM.
        bytes[44..]
            .chunks_exact(2)
            .map(|b| f32::from(i16::from_le_bytes([b[0], b[1]])) / 32768.0)
            .collect()
    }

    #[test]
    fn detects_real_speech_and_rejects_silence() {
        // Arrange — a known speech clip and the model under test
        let speech = read_fixture_speech();
        let mut vad = SileroVad::load().expect("bundled Silero model should load");

        // Act — peak probability over silence, then over the speech clip
        let mut silence_peak = 0.0f32;
        for _ in 0..20 {
            silence_peak = silence_peak.max(vad.predict(&vec![0.0; SILERO_FRAME_SIZE]));
        }
        vad.reset();
        let mut speech_peak = 0.0f32;
        for chunk in speech.chunks(SILERO_FRAME_SIZE) {
            speech_peak = speech_peak.max(vad.predict(chunk));
        }

        // Assert — the model must clearly fire on speech and not on silence.
        // This guards against a silently-broken model/inference (the silence
        // check alone passes even for a model that never detects anything).
        assert!(
            silence_peak < 0.3,
            "silence peaked at {silence_peak}, expected < 0.3"
        );
        assert!(
            speech_peak > 0.8,
            "speech only peaked at {speech_peak}, expected > 0.8"
        );
    }

    #[test]
    fn reset_zeroes_recurrent_state() {
        // Arrange — run a noisy frame to perturb the recurrent state
        let mut vad = SileroVad::load().expect("bundled Silero model should load");
        let noise: Vec<f32> = (0..SILERO_FRAME_SIZE)
            .map(|i| if i % 2 == 0 { 0.3 } else { -0.3 })
            .collect();
        let _ = vad.predict(&noise);

        // Act
        vad.reset();

        // Assert
        assert!(vad.state.iter().all(|&s| s == 0.0));
    }
}
