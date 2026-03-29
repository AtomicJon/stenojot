//! ONNX-based STT backend using the `transcribe-rs` crate.
//!
//! Provides a unified backend for ONNX models (Parakeet, Moonshine,
//! SenseVoice) through the `transcribe-rs` `SpeechModel` trait, wrapped
//! in our [`SttBackend`] interface.

use std::path::Path;

use transcribe_rs::onnx::Quantization;
use transcribe_rs::{SpeechModel, TranscribeOptions};

use super::engine::{SttBackend, SttEngine};

/// ONNX-based STT backend wrapping a `transcribe-rs` model.
///
/// Supports Parakeet, Moonshine, and SenseVoice models loaded from
/// a local directory containing ONNX model files.
pub struct OnnxBackend {
    /// The loaded speech model (type-erased via `Box<dyn SpeechModel>`).
    model: Box<dyn SpeechModel>,
}

impl OnnxBackend {
    /// Load an ONNX model from the given directory.
    ///
    /// The `engine` parameter determines which model type to load.
    /// Models are loaded with Int8 quantization by default for the best
    /// balance of speed and accuracy on CPU.
    pub fn load(engine: SttEngine, model_dir: &Path) -> Result<Self, String> {
        let quantization = Quantization::default();

        let model: Box<dyn SpeechModel> = match engine {
            SttEngine::Parakeet => {
                let m =
                    transcribe_rs::onnx::parakeet::ParakeetModel::load(model_dir, &quantization)
                        .map_err(|e| format!("Failed to load Parakeet model: {}", e))?;
                Box::new(m)
            }
            SttEngine::Moonshine => {
                let variant = moonshine_variant_from_path(model_dir);
                let m = transcribe_rs::onnx::moonshine::MoonshineModel::load(
                    model_dir,
                    variant,
                    &quantization,
                )
                .map_err(|e| format!("Failed to load Moonshine model: {}", e))?;
                Box::new(m)
            }
            SttEngine::SenseVoice => {
                let m = transcribe_rs::onnx::sense_voice::SenseVoiceModel::load(
                    model_dir,
                    &quantization,
                )
                .map_err(|e| format!("Failed to load SenseVoice model: {}", e))?;
                Box::new(m)
            }
            SttEngine::Whisper => {
                return Err("Whisper engine should use WhisperBackend, not OnnxBackend".to_string());
            }
        };

        Ok(Self { model })
    }
}

/// Determine the Moonshine variant from the model directory name.
fn moonshine_variant_from_path(
    model_dir: &Path,
) -> transcribe_rs::onnx::moonshine::MoonshineVariant {
    use transcribe_rs::onnx::moonshine::MoonshineVariant;
    let dir_name = model_dir.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if dir_name.contains("base") {
        MoonshineVariant::Base
    } else {
        MoonshineVariant::Tiny
    }
}

impl SttBackend for OnnxBackend {
    fn transcribe(
        &mut self,
        audio: &[f32],
        _initial_prompt: Option<&str>,
    ) -> Result<String, String> {
        let options = TranscribeOptions {
            language: Some("en".to_string()),
            ..Default::default()
        };

        let result = self
            .model
            .transcribe(audio, &options)
            .map_err(|e| format!("ONNX transcription failed: {}", e))?;

        Ok(result.text)
    }
}
