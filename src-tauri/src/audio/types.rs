use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioDevice {
    pub name: String,
    pub id: String,
    pub is_default: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Speaker {
    Me,
    Others,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioLevels {
    pub mic_rms: f32,
    pub system_rms: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct TranscriptSegment {
    pub text: String,
    pub speaker: Speaker,
    pub start_ms: u64,
    pub end_ms: u64,
    pub is_final: bool,
}

#[derive(Debug, Clone)]
pub enum CaptureError {
    DeviceNotFound(String),
    StreamError(String),
    AlreadyRecording,
    NotRecording,
}

impl fmt::Display for CaptureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CaptureError::DeviceNotFound(name) => write!(f, "Device not found: {}", name),
            CaptureError::StreamError(msg) => write!(f, "Stream error: {}", msg),
            CaptureError::AlreadyRecording => write!(f, "Already recording"),
            CaptureError::NotRecording => write!(f, "Not recording"),
        }
    }
}

impl std::error::Error for CaptureError {}

impl From<CaptureError> for String {
    fn from(e: CaptureError) -> String {
        e.to_string()
    }
}
