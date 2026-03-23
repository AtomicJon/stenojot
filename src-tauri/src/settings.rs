//! Persistent application settings, stored as JSON in the app data directory.
//!
//! Settings are loaded on startup and saved automatically whenever a setting
//! changes. The file format is forward-compatible — unknown fields are ignored
//! and missing fields receive sensible defaults via `#[serde(default)]`.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::audio::pipeline;

/// Persisted application settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Preferred microphone device ID.
    pub mic_device_id: Option<String>,
    /// Preferred system audio monitor source ID.
    pub system_device_id: Option<String>,
    /// Microphone gain multiplier (0.1–10.0).
    pub mic_gain: f32,
    /// VAD sensitivity threshold (0.0005–0.1).
    pub vad_threshold: f32,
    /// Custom models directory path (None = use default).
    pub models_dir: Option<String>,
    /// Output directory for meeting transcript files (None = ~/EchoNotes/).
    pub output_dir: Option<String>,
    /// Auto-stop after this many seconds of silence (None = disabled).
    pub silence_timeout_seconds: Option<u32>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            mic_device_id: None,
            system_device_id: None,
            mic_gain: 1.0,
            vad_threshold: pipeline::DEFAULT_VAD_THRESHOLD,
            models_dir: None,
            output_dir: None,
            silence_timeout_seconds: Some(300),
        }
    }
}

/// Default output directory for transcript files.
const DEFAULT_OUTPUT_DIR_NAME: &str = "EchoNotes";

impl Settings {
    /// Resolve the output directory, defaulting to `~/EchoNotes/`.
    pub fn output_dir_resolved(&self) -> PathBuf {
        match &self.output_dir {
            Some(dir) if !dir.is_empty() => PathBuf::from(dir),
            _ => dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("/tmp"))
                .join(DEFAULT_OUTPUT_DIR_NAME),
        }
    }
}

/// Returns the path to the settings JSON file within the given data directory.
fn settings_path(data_dir: &Path) -> PathBuf {
    data_dir.join("settings.json")
}

/// Load settings from disk, falling back to defaults on any error.
///
/// Missing fields in the JSON are filled with defaults thanks to
/// `#[serde(default)]`, so adding new settings later is backward-compatible.
pub fn load(data_dir: &Path) -> Settings {
    let path = settings_path(data_dir);
    match fs::read_to_string(&path) {
        Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
        Err(_) => Settings::default(),
    }
}

/// Save settings to disk as pretty-printed JSON.
///
/// Writes to a temporary file first then renames, so a crash mid-write
/// cannot corrupt the settings file.
pub fn save(data_dir: &Path, settings: &Settings) -> Result<(), String> {
    fs::create_dir_all(data_dir)
        .map_err(|e| format!("Failed to create settings directory: {}", e))?;

    let path = settings_path(data_dir);
    let tmp_path = data_dir.join("settings.json.tmp");

    let json = serde_json::to_string_pretty(settings)
        .map_err(|e| format!("Failed to serialize settings: {}", e))?;

    fs::write(&tmp_path, json)
        .map_err(|e| format!("Failed to write settings file: {}", e))?;

    fs::rename(&tmp_path, &path)
        .map_err(|e| format!("Failed to rename settings file: {}", e))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_returns_defaults_when_file_missing() {
        // Arrange
        let tmp = tempfile::tempdir().unwrap();

        // Act
        let settings = load(tmp.path());

        // Assert
        assert_eq!(settings.mic_gain, 1.0);
        assert_eq!(settings.vad_threshold, pipeline::DEFAULT_VAD_THRESHOLD);
        assert!(settings.mic_device_id.is_none());
        assert!(settings.system_device_id.is_none());
        assert!(settings.models_dir.is_none());
    }

    #[test]
    fn save_and_load_roundtrip() {
        // Arrange
        let tmp = tempfile::tempdir().unwrap();
        let settings = Settings {
            mic_device_id: Some("usb-mic-1".to_string()),
            system_device_id: Some("monitor-0".to_string()),
            mic_gain: 2.5,
            vad_threshold: 0.01,
            models_dir: Some("/custom/models".to_string()),
            output_dir: Some("/custom/output".to_string()),
            silence_timeout_seconds: Some(120),
        };

        // Act
        save(tmp.path(), &settings).unwrap();
        let loaded = load(tmp.path());

        // Assert
        assert_eq!(loaded.mic_device_id.as_deref(), Some("usb-mic-1"));
        assert_eq!(loaded.system_device_id.as_deref(), Some("monitor-0"));
        assert!((loaded.mic_gain - 2.5).abs() < f32::EPSILON);
        assert!((loaded.vad_threshold - 0.01).abs() < f32::EPSILON);
        assert_eq!(loaded.models_dir.as_deref(), Some("/custom/models"));
    }

    #[test]
    fn load_handles_partial_json() {
        // Arrange — only mic_gain present, rest should default
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("settings.json");
        fs::write(&path, r#"{ "mic_gain": 3.0 }"#).unwrap();

        // Act
        let settings = load(tmp.path());

        // Assert
        assert!((settings.mic_gain - 3.0).abs() < f32::EPSILON);
        assert_eq!(settings.vad_threshold, pipeline::DEFAULT_VAD_THRESHOLD);
        assert!(settings.mic_device_id.is_none());
    }

    #[test]
    fn load_handles_corrupt_json() {
        // Arrange
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("settings.json");
        fs::write(&path, "not valid json {{{").unwrap();

        // Act
        let settings = load(tmp.path());

        // Assert — should return defaults, not panic
        assert_eq!(settings.mic_gain, 1.0);
    }

    #[test]
    fn save_creates_directory_if_missing() {
        // Arrange
        let tmp = tempfile::tempdir().unwrap();
        let nested = tmp.path().join("deep").join("nested");
        let settings = Settings::default();

        // Act
        let result = save(&nested, &settings);

        // Assert
        assert!(result.is_ok());
        assert!(nested.join("settings.json").exists());
    }

    #[test]
    fn load_ignores_unknown_fields() {
        // Arrange — JSON has a field that doesn't exist in the struct
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("settings.json");
        fs::write(
            &path,
            r#"{ "mic_gain": 2.0, "future_field": "hello" }"#,
        )
        .unwrap();

        // Act
        let settings = load(tmp.path());

        // Assert
        assert!((settings.mic_gain - 2.0).abs() < f32::EPSILON);
    }
}
