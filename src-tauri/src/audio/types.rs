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
    /// True if the recording is currently paused.
    pub is_paused: bool,
    /// True if the worker auto-stopped due to silence timeout.
    pub auto_stopped: bool,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_error_display_device_not_found() {
        // Arrange
        let err = CaptureError::DeviceNotFound("mic-1".to_string());

        // Act
        let msg = err.to_string();

        // Assert
        assert_eq!(msg, "Device not found: mic-1");
    }

    #[test]
    fn capture_error_display_stream_error() {
        // Arrange
        let err = CaptureError::StreamError("timeout".to_string());

        // Act
        let msg = err.to_string();

        // Assert
        assert_eq!(msg, "Stream error: timeout");
    }

    #[test]
    fn capture_error_display_already_recording() {
        // Arrange
        let err = CaptureError::AlreadyRecording;

        // Act
        let msg = err.to_string();

        // Assert
        assert_eq!(msg, "Already recording");
    }

    #[test]
    fn capture_error_display_not_recording() {
        // Arrange
        let err = CaptureError::NotRecording;

        // Act
        let msg = err.to_string();

        // Assert
        assert_eq!(msg, "Not recording");
    }

    #[test]
    fn capture_error_converts_to_string() {
        // Arrange
        let err = CaptureError::AlreadyRecording;

        // Act
        let s: String = err.into();

        // Assert
        assert_eq!(s, "Already recording");
    }

    #[test]
    fn speaker_serializes_correctly() {
        // Arrange
        let me = Speaker::Me;
        let others = Speaker::Others;

        // Act
        let me_json = serde_json::to_string(&me).unwrap();
        let others_json = serde_json::to_string(&others).unwrap();

        // Assert
        assert_eq!(me_json, "\"Me\"");
        assert_eq!(others_json, "\"Others\"");
    }
}
