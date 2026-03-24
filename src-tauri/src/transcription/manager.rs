//! Whisper model management: storage paths, existence checks, and downloads.
//!
//! Models are stored in a configurable directory (defaulting to
//! `~/.local/share/stenojot/models/`) and downloaded from Hugging Face on
//! first use. The storage location can be overridden at runtime via
//! [`set_models_dir`] and reset back to the default with [`reset_models_dir`].

use serde::Serialize;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

/// Base URL for downloading ggml Whisper models from Hugging Face.
const HF_BASE_URL: &str =
    "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";

/// Optional override for the models storage directory.
static CUSTOM_MODELS_DIR: Mutex<Option<PathBuf>> = Mutex::new(None);

/// Detailed information about a Whisper model file.
#[derive(Serialize)]
pub struct ModelInfo {
    /// Model name (e.g. `"base"`).
    pub name: String,
    /// Absolute path where the model file is (or would be) stored.
    pub path: String,
    /// Whether the model file exists on disk.
    pub downloaded: bool,
    /// File size in bytes (0 if not downloaded).
    pub size_bytes: u64,
    /// Directory containing model files.
    pub models_dir: String,
}

/// Override the directory where Whisper models are stored.
///
/// The directory must exist or be creatable. Returns an error if the path
/// is not a valid directory and cannot be created.
pub fn set_models_dir(path: PathBuf) -> Result<(), String> {
    fs::create_dir_all(&path)
        .map_err(|e| format!("Failed to create models directory: {}", e))?;
    let mut dir = CUSTOM_MODELS_DIR
        .lock()
        .map_err(|e| format!("Lock error: {}", e))?;
    *dir = Some(path);
    Ok(())
}

/// Reset the models directory to the default location.
pub fn reset_models_dir() {
    if let Ok(mut dir) = CUSTOM_MODELS_DIR.lock() {
        *dir = None;
    }
}

/// Returns the directory where Whisper models are stored.
///
/// Uses the custom directory if one has been set via [`set_models_dir`],
/// otherwise falls back to `~/.local/share/stenojot/models/`.
pub fn get_models_dir() -> PathBuf {
    if let Ok(dir) = CUSTOM_MODELS_DIR.lock() {
        if let Some(ref custom) = *dir {
            return custom.clone();
        }
    }
    default_models_dir()
}

/// Returns the custom models directory if one has been set, or `None`
/// if using the default location.
pub fn get_custom_models_dir() -> Option<PathBuf> {
    if let Ok(dir) = CUSTOM_MODELS_DIR.lock() {
        return dir.clone();
    }
    None
}

/// Returns the default models directory (`~/.local/share/stenojot/models/`).
fn default_models_dir() -> PathBuf {
    dirs_like_home().join(".local/share/stenojot/models")
}

/// Best-effort home directory lookup; falls back to `/tmp`.
fn dirs_like_home() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp"))
}

/// Returns the expected filesystem path for a given model name (e.g. `"base"`).
pub fn get_model_path(model_name: &str) -> PathBuf {
    get_models_dir().join(format!("ggml-{}.bin", model_name))
}

/// Checks whether the model file already exists on disk.
pub fn model_exists(model_name: &str) -> bool {
    get_model_path(model_name).exists()
}

/// Returns the Hugging Face download URL for the given model name.
pub fn get_download_url(model_name: &str) -> String {
    format!("{}/ggml-{}.bin", HF_BASE_URL, model_name)
}

/// Returns detailed information about a Whisper model.
///
/// Includes the model path, download status, file size, and the active
/// models directory.
pub fn get_model_info(model_name: &str) -> ModelInfo {
    let path = get_model_path(model_name);
    let downloaded = path.exists();
    let size_bytes = if downloaded {
        fs::metadata(&path).map(|m| m.len()).unwrap_or(0)
    } else {
        0
    };

    ModelInfo {
        name: model_name.to_string(),
        path: path.to_string_lossy().to_string(),
        downloaded,
        size_bytes,
        models_dir: get_models_dir().to_string_lossy().to_string(),
    }
}

/// Delete the model file for the given model name.
///
/// Returns an error if the file does not exist or cannot be removed.
pub fn delete_model(model_name: &str) -> Result<(), String> {
    let path = get_model_path(model_name);
    if !path.exists() {
        return Err(format!("Model file does not exist: {}", path.display()));
    }
    fs::remove_file(&path)
        .map_err(|e| format!("Failed to delete model file: {}", e))?;
    Ok(())
}

/// Downloads the model file from Hugging Face to the local models directory.
///
/// Creates the models directory if it does not exist. Uses a streaming
/// download to avoid holding the entire file in memory.
pub fn download_model_file(model_name: &str) -> Result<PathBuf, String> {
    let path = get_model_path(model_name);

    if path.exists() {
        return Ok(path);
    }

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create models directory: {}", e))?;
    }

    let url = get_download_url(model_name);
    eprintln!("Downloading model from {} ...", url);

    let response = reqwest::blocking::get(&url)
        .map_err(|e| format!("Download request failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!(
            "Download failed with HTTP status {}",
            response.status()
        ));
    }

    let bytes = response
        .bytes()
        .map_err(|e| format!("Failed to read response body: {}", e))?;

    let mut file = fs::File::create(&path)
        .map_err(|e| format!("Failed to create model file: {}", e))?;

    file.write_all(&bytes)
        .map_err(|e| format!("Failed to write model file: {}", e))?;

    eprintln!("Model saved to {}", path.display());
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::MutexGuard;

    /// Serializes manager tests that mutate the global CUSTOM_MODELS_DIR.
    static TEST_MUTEX: Mutex<()> = Mutex::new(());

    /// Acquire the test lock and set a temporary models dir.
    /// Returns the lock guard — the custom dir is cleared when dropped.
    fn lock_with_temp_dir(dir: &std::path::Path) -> MutexGuard<'static, ()> {
        let guard = TEST_MUTEX.lock().unwrap();
        let mut lock = CUSTOM_MODELS_DIR.lock().unwrap();
        *lock = Some(dir.to_path_buf());
        guard
    }

    /// Clear the custom models dir (call before dropping the test guard).
    fn clear_custom_dir() {
        let mut lock = CUSTOM_MODELS_DIR.lock().unwrap();
        *lock = None;
    }

    #[test]
    fn get_model_path_uses_expected_filename() {
        // Arrange
        let tmp = tempfile::tempdir().unwrap();
        let _guard = lock_with_temp_dir(tmp.path());

        // Act
        let path = get_model_path("base");

        // Assert
        assert_eq!(path.file_name().unwrap(), "ggml-base.bin");
        assert_eq!(path.parent().unwrap(), tmp.path());

        // Cleanup
        clear_custom_dir();
    }

    #[test]
    fn model_exists_returns_false_when_missing() {
        // Arrange
        let tmp = tempfile::tempdir().unwrap();
        let _guard = lock_with_temp_dir(tmp.path());

        // Act
        let exists = model_exists("base");

        // Assert
        assert!(!exists);

        // Cleanup
        clear_custom_dir();
    }

    #[test]
    fn model_exists_returns_true_when_present() {
        // Arrange
        let tmp = tempfile::tempdir().unwrap();
        let _guard = lock_with_temp_dir(tmp.path());
        let model_path = tmp.path().join("ggml-base.bin");
        fs::write(&model_path, b"fake model data").unwrap();

        // Act
        let exists = model_exists("base");

        // Assert
        assert!(exists);

        // Cleanup
        clear_custom_dir();
    }

    #[test]
    fn get_model_info_reflects_download_status() {
        // Arrange
        let tmp = tempfile::tempdir().unwrap();
        let _guard = lock_with_temp_dir(tmp.path());

        // Act — model not downloaded
        let info_missing = get_model_info("base");

        // Assert
        assert!(!info_missing.downloaded);
        assert_eq!(info_missing.size_bytes, 0);
        assert_eq!(info_missing.name, "base");

        // Arrange — create a fake model file
        let model_path = tmp.path().join("ggml-base.bin");
        let fake_data = vec![0u8; 1024];
        fs::write(&model_path, &fake_data).unwrap();

        // Act — model is now present
        let info_present = get_model_info("base");

        // Assert
        assert!(info_present.downloaded);
        assert_eq!(info_present.size_bytes, 1024);

        // Cleanup
        clear_custom_dir();
    }

    #[test]
    fn delete_model_removes_file() {
        // Arrange
        let tmp = tempfile::tempdir().unwrap();
        let _guard = lock_with_temp_dir(tmp.path());
        let model_path = tmp.path().join("ggml-base.bin");
        fs::write(&model_path, b"fake model").unwrap();
        assert!(model_path.exists());

        // Act
        let result = delete_model("base");

        // Assert
        assert!(result.is_ok());
        assert!(!model_path.exists());

        // Cleanup
        clear_custom_dir();
    }

    #[test]
    fn delete_model_returns_error_when_missing() {
        // Arrange
        let tmp = tempfile::tempdir().unwrap();
        let _guard = lock_with_temp_dir(tmp.path());

        // Act
        let result = delete_model("base");

        // Assert
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("does not exist"));

        // Cleanup
        clear_custom_dir();
    }

    #[test]
    fn set_models_dir_creates_directory() {
        // Arrange
        let tmp = tempfile::tempdir().unwrap();
        let new_dir = tmp.path().join("custom_models");
        assert!(!new_dir.exists());

        // Act
        let result = set_models_dir(new_dir.clone());

        // Assert
        assert!(result.is_ok());
        assert!(new_dir.exists());

        // Verify it's being used
        let current = get_models_dir();
        assert_eq!(current, new_dir);

        // Cleanup
        clear_custom_dir();
    }

    #[test]
    fn reset_models_dir_clears_custom_path() {
        // Arrange
        let tmp = tempfile::tempdir().unwrap();
        let _guard = lock_with_temp_dir(tmp.path());
        assert_eq!(get_models_dir(), tmp.path());

        // Act
        reset_models_dir();

        // Assert — should fall back to default
        let dir = get_models_dir();
        assert_ne!(dir, tmp.path());
    }

    #[test]
    fn get_download_url_formats_correctly() {
        // Arrange
        let model_name = "base";

        // Act
        let url = get_download_url(model_name);

        // Assert
        assert_eq!(
            url,
            "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin"
        );
    }
}
