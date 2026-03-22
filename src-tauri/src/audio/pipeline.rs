use ringbuf::traits::Consumer as _;
use ringbuf::HeapCons;
use rubato::{Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction};

const TARGET_SAMPLE_RATE: u32 = 16_000;
const CHUNK_SIZE: usize = 1024;
const VAD_THRESHOLD: f32 = 0.01;

/// Resamples audio to 16kHz mono and applies simple energy-based VAD.
pub struct AudioPipeline {
    mic_consumer: Option<HeapCons<f32>>,
    system_consumer: Option<HeapCons<f32>>,
}

impl AudioPipeline {
    pub fn new() -> Self {
        Self {
            mic_consumer: None,
            system_consumer: None,
        }
    }

    pub fn set_mic_consumer(&mut self, consumer: HeapCons<f32>) {
        self.mic_consumer = Some(consumer);
    }

    pub fn set_system_consumer(&mut self, consumer: HeapCons<f32>) {
        self.system_consumer = Some(consumer);
    }

    /// Drain both ring buffers to prevent overflow.
    /// In Phase 2 this will feed into transcription; for now just discard.
    pub fn drain_buffers(&mut self) {
        if let Some(ref mut consumer) = self.mic_consumer {
            while consumer.try_pop().is_some() {}
        }
        if let Some(ref mut consumer) = self.system_consumer {
            while consumer.try_pop().is_some() {}
        }
    }

    /// Remove consumers when recording stops.
    pub fn clear_consumers(&mut self) {
        self.mic_consumer = None;
        self.system_consumer = None;
    }
}

/// Resample audio from the source sample rate and channel count to 16kHz mono.
///
/// Returns the resampled mono audio at 16kHz.
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
/// Returns true if the RMS energy of the chunk exceeds the threshold.
pub fn is_speech(chunk: &[f32]) -> bool {
    if chunk.is_empty() {
        return false;
    }
    let sum_sq: f32 = chunk.iter().map(|&s| s * s).sum();
    let rms = (sum_sq / chunk.len() as f32).sqrt();
    rms > VAD_THRESHOLD
}
