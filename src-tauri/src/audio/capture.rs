use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, SampleFormat, Stream, StreamConfig};
use ringbuf::traits::Split;
use ringbuf::HeapRb;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use super::types::{AudioDevice, CaptureError};

/// List all available input devices.
/// On Linux with PipeWire, monitor sources appear as input devices.
pub fn list_input_devices() -> Result<Vec<AudioDevice>, CaptureError> {
    let host = cpal::default_host();

    let default_device_name = host
        .default_input_device()
        .and_then(|d| d.name().ok());

    let devices = host
        .input_devices()
        .map_err(|e| CaptureError::StreamError(format!("Failed to enumerate devices: {}", e)))?;

    let mut result = Vec::new();
    for device in devices {
        let name = match device.name() {
            Ok(n) => n,
            Err(_) => continue,
        };
        let is_default = default_device_name
            .as_ref()
            .map(|d| d == &name)
            .unwrap_or(false);
        result.push(AudioDevice {
            id: name.clone(),
            name,
            is_default,
        });
    }

    Ok(result)
}

/// Find an input device by its ID (device name).
fn find_device_by_id(device_id: &str) -> Result<Device, CaptureError> {
    let host = cpal::default_host();
    let devices = host
        .input_devices()
        .map_err(|e| CaptureError::StreamError(format!("Failed to enumerate devices: {}", e)))?;

    for device in devices {
        if let Ok(name) = device.name() {
            if name == device_id {
                return Ok(device);
            }
        }
    }

    Err(CaptureError::DeviceNotFound(device_id.to_string()))
}

/// Information about a started capture stream.
pub struct CaptureHandle {
    pub stream: Stream,
    pub sample_rate: u32,
    pub channels: u16,
    pub rms_level: Arc<AtomicU32>,
    pub consumer: ringbuf::HeapCons<f32>,
}

/// Start capturing audio from the specified device.
///
/// The `gain` parameter is a shared atomic f32 (stored as u32 bits) that
/// scales the captured samples in real time. A value of 1.0 is unity gain.
/// It can be changed while recording to adjust sensitivity on the fly.
pub fn start_capture(
    device_id: &str,
    gain: Arc<AtomicU32>,
) -> Result<CaptureHandle, CaptureError> {
    let device = find_device_by_id(device_id)?;

    let config = device
        .default_input_config()
        .map_err(|e| CaptureError::StreamError(format!("No default input config: {}", e)))?;

    let sample_rate = config.sample_rate().0;
    let channels = config.channels();

    // Ring buffer: ~1 second of audio at the source sample rate
    let capacity = (sample_rate as usize) * (channels as usize);
    let rb = HeapRb::<f32>::new(capacity);
    let (mut producer, consumer) = rb.split();

    let rms_level = Arc::new(AtomicU32::new(0u32));
    let rms_level_clone = Arc::clone(&rms_level);

    // Build a stream config from the default config
    let stream_config: StreamConfig = config.config();

    let err_fn = |err: cpal::StreamError| {
        eprintln!("Audio stream error: {}", err);
    };

    // Each match arm moves producer/rms_level_clone/gain into a closure,
    // so we must prepare separate clones for each arm up front.
    let stream = match config.sample_format() {
        SampleFormat::F32 => device.build_input_stream(
            &stream_config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                capture_callback_with_gain(data, &mut producer, &rms_level_clone, &gain);
            },
            err_fn,
            None,
        ),
        SampleFormat::I16 => device.build_input_stream(
            &stream_config,
            move |data: &[i16], _: &cpal::InputCallbackInfo| {
                let float_data: Vec<f32> = data
                    .iter()
                    .map(|&s| s as f32 / i16::MAX as f32)
                    .collect();
                capture_callback_with_gain(&float_data, &mut producer, &rms_level_clone, &gain);
            },
            err_fn,
            None,
        ),
        SampleFormat::U16 => device.build_input_stream(
            &stream_config,
            move |data: &[u16], _: &cpal::InputCallbackInfo| {
                let float_data: Vec<f32> = data
                    .iter()
                    .map(|&s| (s as f32 / u16::MAX as f32) * 2.0 - 1.0)
                    .collect();
                capture_callback_with_gain(&float_data, &mut producer, &rms_level_clone, &gain);
            },
            err_fn,
            None,
        ),
        format => {
            return Err(CaptureError::StreamError(format!(
                "Unsupported sample format: {:?}",
                format
            )));
        }
    }
    .map_err(|e| CaptureError::StreamError(format!("Failed to build stream: {}", e)))?;

    stream
        .play()
        .map_err(|e| CaptureError::StreamError(format!("Failed to start stream: {}", e)))?;

    Ok(CaptureHandle {
        stream,
        sample_rate,
        channels,
        rms_level,
        consumer,
    })
}

/// Callback invoked by cpal for each audio buffer.
/// Applies gain, pushes samples into the ring buffer, and updates RMS level.
fn capture_callback_with_gain(
    data: &[f32],
    producer: &mut ringbuf::HeapProd<f32>,
    rms_level: &Arc<AtomicU32>,
    gain: &Arc<AtomicU32>,
) {
    use ringbuf::traits::Producer as _;

    let g = f32::from_bits(gain.load(Ordering::Relaxed));

    // Compute RMS and push gained samples into ring buffer
    let mut sum_sq: f32 = 0.0;
    for &sample in data {
        let gained = (sample * g).clamp(-1.0, 1.0);
        sum_sq += gained * gained;
        let _ = producer.try_push(gained);
    }

    if !data.is_empty() {
        let rms = (sum_sq / data.len() as f32).sqrt();
        rms_level.store(rms.to_bits(), Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ringbuf::traits::{Consumer as _, Split};
    use ringbuf::HeapRb;

    // ── capture_callback_with_gain ───────────────────

    #[test]
    fn callback_applies_unity_gain() {
        // Arrange
        let rb = HeapRb::<f32>::new(1024);
        let (mut producer, mut consumer) = rb.split();
        let rms_level = Arc::new(AtomicU32::new(0));
        let gain = Arc::new(AtomicU32::new(1.0f32.to_bits()));
        let data = vec![0.5f32; 100];

        // Act
        capture_callback_with_gain(&data, &mut producer, &rms_level, &gain);

        // Assert — samples should pass through unchanged
        let mut output = Vec::new();
        while let Some(s) = consumer.try_pop() {
            output.push(s);
        }
        assert_eq!(output.len(), 100);
        for &s in &output {
            assert!((s - 0.5).abs() < 1e-6);
        }
    }

    #[test]
    fn callback_applies_gain_multiplier() {
        // Arrange
        let rb = HeapRb::<f32>::new(1024);
        let (mut producer, mut consumer) = rb.split();
        let rms_level = Arc::new(AtomicU32::new(0));
        let gain = Arc::new(AtomicU32::new(2.0f32.to_bits()));
        let data = vec![0.3f32; 100];

        // Act
        capture_callback_with_gain(&data, &mut producer, &rms_level, &gain);

        // Assert — 0.3 * 2.0 = 0.6
        let mut output = Vec::new();
        while let Some(s) = consumer.try_pop() {
            output.push(s);
        }
        for &s in &output {
            assert!((s - 0.6).abs() < 1e-6);
        }
    }

    #[test]
    fn callback_clamps_gained_samples_to_minus_one_to_one() {
        // Arrange — high gain should clip
        let rb = HeapRb::<f32>::new(1024);
        let (mut producer, mut consumer) = rb.split();
        let rms_level = Arc::new(AtomicU32::new(0));
        let gain = Arc::new(AtomicU32::new(10.0f32.to_bits()));
        let data = vec![0.5f32; 50];

        // Act
        capture_callback_with_gain(&data, &mut producer, &rms_level, &gain);

        // Assert — 0.5 * 10.0 = 5.0, clamped to 1.0
        let mut output = Vec::new();
        while let Some(s) = consumer.try_pop() {
            output.push(s);
        }
        for &s in &output {
            assert!((s - 1.0).abs() < 1e-6);
        }
    }

    #[test]
    fn callback_updates_rms_level() {
        // Arrange
        let rb = HeapRb::<f32>::new(1024);
        let (mut producer, _consumer) = rb.split();
        let rms_level = Arc::new(AtomicU32::new(0));
        let gain = Arc::new(AtomicU32::new(1.0f32.to_bits()));
        // Constant 0.5 → RMS = 0.5
        let data = vec![0.5f32; 100];

        // Act
        capture_callback_with_gain(&data, &mut producer, &rms_level, &gain);

        // Assert
        let stored_rms = f32::from_bits(rms_level.load(Ordering::Relaxed));
        assert!((stored_rms - 0.5).abs() < 1e-3);
    }

    #[test]
    fn callback_does_not_update_rms_for_empty_data() {
        // Arrange
        let rb = HeapRb::<f32>::new(1024);
        let (mut producer, _consumer) = rb.split();
        let sentinel = 42u32;
        let rms_level = Arc::new(AtomicU32::new(sentinel));
        let gain = Arc::new(AtomicU32::new(1.0f32.to_bits()));
        let data: Vec<f32> = vec![];

        // Act
        capture_callback_with_gain(&data, &mut producer, &rms_level, &gain);

        // Assert — sentinel value should be unchanged
        assert_eq!(rms_level.load(Ordering::Relaxed), sentinel);
    }
}
