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
