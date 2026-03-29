//! STT engine abstraction with trait-based backend interface.
//!
//! Defines the core [`SttBackend`] trait that all speech-to-text engines
//! must implement, along with the [`SttEngine`] enum for identifying
//! available engines and a factory function for creating the appropriate
//! backend based on user settings.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::Path;

/// Identifies a speech-to-text engine.
///
/// Each variant corresponds to a different inference runtime and model
/// format. The default engine is Whisper (GGML via whisper.cpp).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SttEngine {
    /// OpenAI Whisper via whisper.cpp (GGML format).
    #[default]
    Whisper,
    /// NVIDIA Parakeet TDT via ONNX Runtime.
    Parakeet,
    /// Moonshine v2 via ONNX Runtime.
    Moonshine,
    /// Alibaba SenseVoice via ONNX Runtime.
    #[serde(rename = "sensevoice")]
    SenseVoice,
}

impl fmt::Display for SttEngine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SttEngine::Whisper => write!(f, "whisper"),
            SttEngine::Parakeet => write!(f, "parakeet"),
            SttEngine::Moonshine => write!(f, "moonshine"),
            SttEngine::SenseVoice => write!(f, "sensevoice"),
        }
    }
}

/// Parse a string into an [`SttEngine`], defaulting to Whisper for
/// unrecognized values.
pub fn parse_engine(s: &str) -> SttEngine {
    match s.to_lowercase().as_str() {
        "parakeet" => SttEngine::Parakeet,
        "moonshine" => SttEngine::Moonshine,
        "sensevoice" => SttEngine::SenseVoice,
        _ => SttEngine::Whisper,
    }
}

/// Trait for speech-to-text backend implementations.
///
/// Each engine (Whisper, Parakeet, Moonshine, SenseVoice) implements this
/// trait to provide a uniform interface for transcribing audio buffers.
/// Implementations run on the transcription worker thread.
pub trait SttBackend: Send {
    /// Transcribe a 16 kHz mono f32 audio buffer to text.
    ///
    /// The `initial_prompt` parameter is an optional hint containing domain
    /// terms or names to improve recognition (supported by Whisper; other
    /// engines may ignore it).
    fn transcribe(&mut self, audio: &[f32], initial_prompt: Option<&str>)
        -> Result<String, String>;
}

/// Create the appropriate STT backend for the given engine and model path.
///
/// Loads the model from disk and returns a boxed trait object ready for
/// transcription calls.
pub fn create_backend(engine: SttEngine, model_path: &Path) -> Result<Box<dyn SttBackend>, String> {
    match engine {
        SttEngine::Whisper => {
            let backend = super::whisper_backend::WhisperBackend::load(model_path)?;
            Ok(Box::new(backend))
        }
        SttEngine::Parakeet | SttEngine::Moonshine | SttEngine::SenseVoice => {
            let backend = super::onnx_backend::OnnxBackend::load(engine, model_path)?;
            Ok(Box::new(backend))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_engine_returns_whisper_for_unknown() {
        // Arrange
        let input = "unknown_engine";

        // Act
        let result = parse_engine(input);

        // Assert
        assert_eq!(result, SttEngine::Whisper);
    }

    #[test]
    fn parse_engine_case_insensitive() {
        // Arrange / Act / Assert
        assert_eq!(parse_engine("Parakeet"), SttEngine::Parakeet);
        assert_eq!(parse_engine("MOONSHINE"), SttEngine::Moonshine);
        assert_eq!(parse_engine("SenseVoice"), SttEngine::SenseVoice);
        assert_eq!(parse_engine("WHISPER"), SttEngine::Whisper);
    }

    #[test]
    fn stt_engine_display() {
        // Arrange / Act / Assert
        assert_eq!(SttEngine::Whisper.to_string(), "whisper");
        assert_eq!(SttEngine::Parakeet.to_string(), "parakeet");
        assert_eq!(SttEngine::Moonshine.to_string(), "moonshine");
        assert_eq!(SttEngine::SenseVoice.to_string(), "sensevoice");
    }

    #[test]
    fn stt_engine_default_is_whisper() {
        // Arrange / Act
        let engine = SttEngine::default();

        // Assert
        assert_eq!(engine, SttEngine::Whisper);
    }

    #[test]
    fn stt_engine_serde_roundtrip() {
        // Arrange
        let engines = vec![
            SttEngine::Whisper,
            SttEngine::Parakeet,
            SttEngine::Moonshine,
            SttEngine::SenseVoice,
        ];

        for engine in engines {
            // Act
            let json = serde_json::to_string(&engine).unwrap();
            let deserialized: SttEngine = serde_json::from_str(&json).unwrap();

            // Assert
            assert_eq!(engine, deserialized);
        }
    }
}
