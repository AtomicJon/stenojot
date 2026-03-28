//! Stub system audio capture for platforms without PulseAudio.
//!
//! On non-Linux platforms, system audio capture is not yet supported.
//! This module provides the same public API as the PulseAudio implementation
//! so the rest of the codebase compiles without `#[cfg]` guards everywhere.

use std::sync::atomic::{AtomicBool, AtomicU32};
use std::sync::Arc;

use super::types::{AudioDevice, CaptureError};

/// List available system audio monitor sources.
///
/// Returns an empty list on platforms without PulseAudio support.
pub fn list_monitor_sources() -> Result<Vec<AudioDevice>, CaptureError> {
    Ok(Vec::new())
}

/// Handle for a running system audio capture thread.
pub struct SystemCaptureHandle {
    /// Current RMS level (f32 bits stored as u32).
    pub rms_level: Arc<AtomicU32>,
    /// Consumer end of the ring buffer for reading captured samples.
    pub consumer: Option<ringbuf::HeapCons<f32>>,
    /// Sample rate of the capture stream.
    pub sample_rate: u32,
    /// Number of channels in the capture stream.
    pub channels: u16,
    _running: Arc<AtomicBool>,
}

impl SystemCaptureHandle {
    /// Signal the capture thread to stop and wait for it to finish.
    pub fn stop(&mut self) {
        // No-op on unsupported platforms
    }
}

/// Start capturing system audio from the specified source.
///
/// Always returns an error on platforms without PulseAudio support.
pub fn start_system_capture(_source_name: &str) -> Result<SystemCaptureHandle, CaptureError> {
    Err(CaptureError::StreamError(
        "System audio capture is not supported on this platform".to_string(),
    ))
}
