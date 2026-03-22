//! System audio capture via PulseAudio/PipeWire monitor sources.
//!
//! On Linux, ALSA (used by cpal) does not expose PulseAudio monitor sources.
//! This module uses the PulseAudio Simple API to capture from monitor sources,
//! which mirror system audio output — i.e., what you hear through speakers.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::thread;

use libpulse_binding::sample::{Format, Spec};
use libpulse_binding::stream::Direction;
use libpulse_simple_binding::Simple;
use ringbuf::traits::{Producer as _, Split};
use ringbuf::HeapRb;

use super::types::{AudioDevice, CaptureError};

/// Sample rate used for PulseAudio capture.
/// We request 48 kHz to match typical system audio output.
const CAPTURE_SAMPLE_RATE: u32 = 48_000;

/// Number of channels for PulseAudio capture.
const CAPTURE_CHANNELS: u8 = 2;

/// Number of f32 samples per read call.
/// At 48 kHz stereo, 4800 samples ≈ 50 ms of audio.
const READ_SAMPLES: usize = 4800;

/// List available PulseAudio/PipeWire monitor sources.
///
/// Runs `pactl list short sources` and filters for entries whose name
/// contains `.monitor`. These sources capture system audio output.
pub fn list_monitor_sources() -> Result<Vec<AudioDevice>, CaptureError> {
    let output = std::process::Command::new("pactl")
        .args(["list", "short", "sources"])
        .output()
        .map_err(|e| CaptureError::StreamError(format!("Failed to run pactl: {}", e)))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut devices = Vec::new();

    for line in stdout.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 2 {
            let source_name = parts[1].to_string();
            if source_name.contains(".monitor") {
                // Build a readable display name from the source name
                let display_name = source_name
                    .replace("alsa_output.", "")
                    .replace(".monitor", "")
                    .replace('.', " ")
                    .replace('-', " ");
                devices.push(AudioDevice {
                    id: source_name,
                    name: display_name,
                    is_default: false,
                });
            }
        }
    }

    // Mark the first monitor as default if any exist
    if let Some(first) = devices.first_mut() {
        first.is_default = true;
    }

    Ok(devices)
}

/// Handle for a running system audio capture thread.
pub struct SystemCaptureHandle {
    /// Shared flag to signal the capture thread to stop.
    running: Arc<AtomicBool>,
    /// Join handle for the capture thread.
    handle: Option<thread::JoinHandle<()>>,
    /// Current RMS level (f32 bits stored as u32).
    pub rms_level: Arc<AtomicU32>,
    /// Consumer end of the ring buffer for reading captured samples.
    /// Wrapped in Option so it can be taken by the transcription worker.
    pub consumer: Option<ringbuf::HeapCons<f32>>,
    /// Sample rate of the capture stream.
    pub sample_rate: u32,
    /// Number of channels in the capture stream.
    pub channels: u16,
}

impl SystemCaptureHandle {
    /// Signal the capture thread to stop and wait for it to finish.
    pub fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

/// Start capturing system audio from the specified PulseAudio monitor source.
///
/// Spawns a background thread that reads from the monitor source using the
/// PulseAudio Simple API and pushes f32 samples into a ring buffer.
pub fn start_system_capture(source_name: &str) -> Result<SystemCaptureHandle, CaptureError> {
    let spec = Spec {
        format: Format::F32le,
        channels: CAPTURE_CHANNELS,
        rate: CAPTURE_SAMPLE_RATE,
    };

    if !spec.is_valid() {
        return Err(CaptureError::StreamError(
            "Invalid PulseAudio sample spec".to_string(),
        ));
    }

    let source = source_name.to_string();

    // Verify we can connect to PulseAudio and the source exists
    let simple = Simple::new(
        None,
        "EchoNotes",
        Direction::Record,
        Some(source.as_str()),
        "system-audio-capture",
        &spec,
        None,
        None,
    )
    .map_err(|e| {
        CaptureError::StreamError(format!(
            "Failed to connect to PulseAudio source '{}': {}",
            source, e
        ))
    })?;

    // Ring buffer: ~1 second of audio
    let capacity = (CAPTURE_SAMPLE_RATE as usize) * (CAPTURE_CHANNELS as usize);
    let rb = HeapRb::<f32>::new(capacity);
    let (producer, consumer) = rb.split();

    let rms_level = Arc::new(AtomicU32::new(0u32));
    let running = Arc::new(AtomicBool::new(true));

    let rms_clone = Arc::clone(&rms_level);
    let running_clone = Arc::clone(&running);

    let handle = thread::spawn(move || {
        capture_loop(simple, producer, rms_clone, running_clone);
    });

    Ok(SystemCaptureHandle {
        running,
        handle: Some(handle),
        rms_level,
        consumer: Some(consumer),
        sample_rate: CAPTURE_SAMPLE_RATE,
        channels: CAPTURE_CHANNELS as u16,
    })
}

/// Main capture loop running on a background thread.
///
/// Reads audio from PulseAudio in fixed-size chunks, computes RMS,
/// and pushes samples into the ring buffer.
fn capture_loop(
    simple: Simple,
    mut producer: ringbuf::HeapProd<f32>,
    rms_level: Arc<AtomicU32>,
    running: Arc<AtomicBool>,
) {
    // Buffer for reading raw bytes from PulseAudio (f32 = 4 bytes per sample)
    let mut byte_buf = vec![0u8; READ_SAMPLES * 4];

    while running.load(Ordering::SeqCst) {
        if let Err(e) = simple.read(&mut byte_buf) {
            eprintln!("PulseAudio read error: {}", e);
            break;
        }

        // Convert raw bytes to f32 samples
        let samples: &[f32] = unsafe {
            std::slice::from_raw_parts(byte_buf.as_ptr() as *const f32, READ_SAMPLES)
        };

        // Compute RMS
        if !samples.is_empty() {
            let sum_sq: f32 = samples.iter().map(|&s| s * s).sum();
            let rms = (sum_sq / samples.len() as f32).sqrt();
            rms_level.store(rms.to_bits(), Ordering::Relaxed);
        }

        // Push into ring buffer
        for &sample in samples {
            let _ = producer.try_push(sample);
        }
    }
}
