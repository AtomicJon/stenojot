//! Whisper STT backend using whisper-rs (whisper.cpp GGML models).
//!
//! Wraps `WhisperContext` from the `whisper-rs` crate and implements
//! [`SttBackend`] so the transcription worker can use Whisper through
//! the same trait-based interface as other engines.

use std::path::Path;

use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use super::engine::SttBackend;

/// Whisper backend using whisper.cpp via the `whisper-rs` crate.
///
/// Holds a loaded `WhisperContext` and provides transcription through
/// the [`SttBackend`] trait.
pub struct WhisperBackend {
    /// The loaded Whisper model context.
    ctx: WhisperContext,
}

impl WhisperBackend {
    /// Load a Whisper GGML model from the given path.
    pub fn load(model_path: &Path) -> Result<Self, String> {
        let ctx = WhisperContext::new_with_params(
            model_path.to_str().unwrap_or_default(),
            WhisperContextParameters::default(),
        )
        .map_err(|e| format!("Failed to load Whisper model: {}", e))?;

        Ok(Self { ctx })
    }
}

impl SttBackend for WhisperBackend {
    fn transcribe(
        &mut self,
        audio: &[f32],
        initial_prompt: Option<&str>,
    ) -> Result<String, String> {
        let mut state = self
            .ctx
            .create_state()
            .map_err(|e| format!("Failed to create Whisper state: {}", e))?;

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(Some("en"));
        params.set_print_special(false);
        params.set_print_realtime(false);
        params.set_print_progress(false);

        if let Some(prompt) = initial_prompt {
            params.set_initial_prompt(prompt);
        }

        state
            .full(params, audio)
            .map_err(|e| format!("Whisper inference failed: {}", e))?;

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

        Ok(text_parts.join(" "))
    }
}
