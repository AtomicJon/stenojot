//! Audio processing utilities for resampling and voice activity detection.
//!
//! Provides standalone functions used by the transcription worker to
//! convert captured audio to 16 kHz mono and detect speech segments.

use rubato::{
    Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
};

const TARGET_SAMPLE_RATE: u32 = 16_000;
const CHUNK_SIZE: usize = 1024;
/// Default VAD threshold — used as the initial value when no custom
/// threshold has been configured.
pub const DEFAULT_VAD_THRESHOLD: f32 = 0.005;

/// Resample audio from the source sample rate and channel count to 16 kHz mono.
///
/// Returns the resampled mono audio at 16 kHz.
pub fn process_buffer(input: &[f32], from_sample_rate: u32, from_channels: u16) -> Vec<f32> {
    if input.is_empty() {
        return Vec::new();
    }

    // First, downmix to mono if multichannel
    let mono: Vec<f32> = if from_channels > 1 {
        input
            .chunks(from_channels as usize)
            .map(|frame| {
                let sum: f32 = frame.iter().sum();
                sum / from_channels as f32
            })
            .collect()
    } else {
        input.to_vec()
    };

    // If already at target sample rate, return mono directly
    if from_sample_rate == TARGET_SAMPLE_RATE {
        return mono;
    }

    // Set up rubato resampler
    let resample_ratio = TARGET_SAMPLE_RATE as f64 / from_sample_rate as f64;
    let params = SincInterpolationParameters {
        sinc_len: 256,
        f_cutoff: 0.95,
        interpolation: SincInterpolationType::Linear,
        oversampling_factor: 256,
        window: WindowFunction::BlackmanHarris2,
    };

    let mut resampler = match SincFixedIn::<f32>::new(
        resample_ratio,
        2.0,
        params,
        CHUNK_SIZE,
        1, // mono
    ) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Failed to create resampler: {}", e);
            return mono;
        }
    };

    let mut output = Vec::new();

    // Process in chunks of CHUNK_SIZE
    for chunk in mono.chunks(CHUNK_SIZE) {
        let mut padded = chunk.to_vec();
        if padded.len() < CHUNK_SIZE {
            padded.resize(CHUNK_SIZE, 0.0);
        }

        let input_frames = vec![padded];
        match resampler.process(&input_frames, None) {
            Ok(resampled) => {
                if let Some(channel) = resampled.first() as Option<&Vec<f32>> {
                    output.extend_from_slice(channel);
                }
            }
            Err(e) => {
                eprintln!("Resampling error: {}", e);
            }
        }
    }

    output
}

/// Simple energy-based Voice Activity Detection.
/// Returns true if the RMS energy of the chunk exceeds the given threshold.
pub fn is_speech(chunk: &[f32], threshold: f32) -> bool {
    if chunk.is_empty() {
        return false;
    }
    let sum_sq: f32 = chunk.iter().map(|&s| s * s).sum();
    let rms = (sum_sq / chunk.len() as f32).sqrt();
    rms > threshold
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── is_speech ────────────────────────────────────

    #[test]
    fn is_speech_returns_false_for_empty_chunk() {
        // Arrange
        let chunk: &[f32] = &[];
        let threshold = 0.01;

        // Act
        let result = is_speech(chunk, threshold);

        // Assert
        assert!(!result);
    }

    #[test]
    fn is_speech_detects_loud_signal() {
        // Arrange — constant 0.5 amplitude → RMS = 0.5
        let chunk: Vec<f32> = vec![0.5; 1600];
        let threshold = 0.01;

        // Act
        let result = is_speech(&chunk, threshold);

        // Assert
        assert!(result);
    }

    #[test]
    fn is_speech_rejects_silence() {
        // Arrange — all zeros → RMS = 0
        let chunk: Vec<f32> = vec![0.0; 1600];
        let threshold = 0.01;

        // Act
        let result = is_speech(&chunk, threshold);

        // Assert
        assert!(!result);
    }

    #[test]
    fn is_speech_rejects_signal_below_threshold() {
        // Arrange — constant 0.001 amplitude → RMS = 0.001, threshold 0.01
        let chunk: Vec<f32> = vec![0.001; 1600];
        let threshold = 0.01;

        // Act
        let result = is_speech(&chunk, threshold);

        // Assert
        assert!(!result);
    }

    #[test]
    fn is_speech_respects_custom_threshold() {
        // Arrange — constant 0.005 amplitude, threshold set just below
        let chunk: Vec<f32> = vec![0.005; 1600];
        let threshold = 0.004;

        // Act
        let result = is_speech(&chunk, threshold);

        // Assert
        assert!(result);
    }

    // ── process_buffer ───────────────────────────────

    #[test]
    fn process_buffer_returns_empty_for_empty_input() {
        // Arrange
        let input: &[f32] = &[];

        // Act
        let result = process_buffer(input, 48_000, 2);

        // Assert
        assert!(result.is_empty());
    }

    #[test]
    fn process_buffer_downmixes_stereo_to_mono() {
        // Arrange — stereo signal: left=1.0, right=0.0 alternating
        let input: Vec<f32> = (0..2048)
            .map(|i| if i % 2 == 0 { 1.0 } else { 0.0 })
            .collect();

        // Act — use 16kHz so no resampling occurs, just downmix
        let result = process_buffer(&input, 16_000, 2);

        // Assert — mono average should be 0.5 for each frame
        assert_eq!(result.len(), 1024);
        for &sample in &result {
            assert!((sample - 0.5).abs() < 1e-6);
        }
    }

    #[test]
    fn process_buffer_passes_mono_16k_through() {
        // Arrange — already 16kHz mono
        let input: Vec<f32> = (0..1600).map(|i| (i as f32) / 1600.0).collect();

        // Act
        let result = process_buffer(&input, 16_000, 1);

        // Assert — should be identical
        assert_eq!(result.len(), input.len());
        for (a, b) in result.iter().zip(input.iter()) {
            assert!((a - b).abs() < 1e-6);
        }
    }

    #[test]
    fn process_buffer_resamples_48k_to_16k() {
        // Arrange — 48kHz mono, should produce roughly 1/3 the samples
        let input: Vec<f32> = vec![0.0; 4800];

        // Act
        let result = process_buffer(&input, 48_000, 1);

        // Assert — 4800 * (16000/48000) = 1600, allow some padding tolerance
        let expected_approx = 1600;
        let tolerance = 200;
        assert!(
            (result.len() as i64 - expected_approx as i64).unsigned_abs() < tolerance,
            "Expected ~{} samples, got {}",
            expected_approx,
            result.len()
        );
    }
}
