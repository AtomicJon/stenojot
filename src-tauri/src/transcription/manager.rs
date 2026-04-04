//! Model management: storage paths, existence checks, downloads, and registry.
//!
//! Models are stored in a configurable directory (defaulting to
//! `~/.local/share/stenojot/models/`) and downloaded from Hugging Face on
//! first use. The storage location can be overridden at runtime via
//! [`set_models_dir`] and reset back to the default with [`reset_models_dir`].
//!
//! Whisper models are single GGML files. ONNX models (Parakeet, Moonshine,
//! SenseVoice) are directories containing multiple files managed by the
//! `transcribe-rs` crate.

use serde::Serialize;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::Emitter;

use super::engine::SttEngine;

/// Base URL for downloading ggml Whisper models from Hugging Face.
const HF_BASE_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";

/// Optional override for the models storage directory.
static CUSTOM_MODELS_DIR: Mutex<Option<PathBuf>> = Mutex::new(None);

/// Event payload emitted during model downloads to report progress.
#[derive(Clone, Serialize)]
pub struct DownloadProgress {
    /// Bytes downloaded so far.
    pub bytes_downloaded: u64,
    /// Total file size in bytes (0 if the server did not send Content-Length).
    pub total_bytes: u64,
}

/// Detailed information about a model file or directory.
#[derive(Serialize)]
pub struct ModelInfo {
    /// Model name (e.g. `"base"`).
    pub name: String,
    /// Absolute path where the model file/directory is (or would be) stored.
    pub path: String,
    /// Whether the model file/directory exists on disk.
    pub downloaded: bool,
    /// File size in bytes (0 if not downloaded). For ONNX models this is
    /// the total size of the model directory.
    pub size_bytes: u64,
    /// Directory containing model files.
    pub models_dir: String,
    /// The STT engine this model belongs to.
    pub engine: String,
}

/// A catalog entry describing an available model.
#[derive(Clone, Serialize)]
pub struct ModelEntry {
    /// Model identifier used in settings (e.g. `"base"`, `"parakeet-tdt-0.6b"`).
    pub id: String,
    /// Human-readable label for the UI.
    pub label: String,
    /// The STT engine this model belongs to.
    pub engine: SttEngine,
    /// HuggingFace repository ID for ONNX models (used by `transcribe-rs`).
    /// `None` for Whisper GGML models which use direct URL downloads.
    pub hf_repo: Option<String>,
}

/// Returns the catalog of available models for a given engine.
pub fn get_engine_models(engine: SttEngine) -> Vec<ModelEntry> {
    match engine {
        SttEngine::Whisper => vec![
            ModelEntry {
                id: "tiny".to_string(),
                label: "Tiny (~75 MB — fastest, least accurate)".to_string(),
                engine: SttEngine::Whisper,
                hf_repo: None,
            },
            ModelEntry {
                id: "base".to_string(),
                label: "Base (~142 MB — fast, good accuracy)".to_string(),
                engine: SttEngine::Whisper,
                hf_repo: None,
            },
            ModelEntry {
                id: "small".to_string(),
                label: "Small (~466 MB — balanced)".to_string(),
                engine: SttEngine::Whisper,
                hf_repo: None,
            },
            ModelEntry {
                id: "medium".to_string(),
                label: "Medium (~1.5 GB — slower, high accuracy)".to_string(),
                engine: SttEngine::Whisper,
                hf_repo: None,
            },
            ModelEntry {
                id: "large-v3-turbo".to_string(),
                label: "Large V3 Turbo (~1.6 GB — fast, very accurate)".to_string(),
                engine: SttEngine::Whisper,
                hf_repo: None,
            },
            ModelEntry {
                id: "distil-large-v3.5".to_string(),
                label: "Distil Large V3.5 (~756 MB — 2x faster, near large accuracy)".to_string(),
                engine: SttEngine::Whisper,
                hf_repo: None,
            },
        ],
        SttEngine::Parakeet => vec![ModelEntry {
            id: "parakeet-tdt-0.6b".to_string(),
            label: "Parakeet TDT 0.6B (~670 MB — fastest, excellent accuracy)".to_string(),
            engine: SttEngine::Parakeet,
            hf_repo: Some("istupakov/parakeet-tdt-0.6b-v3-onnx".to_string()),
        }],
        SttEngine::Moonshine => vec![
            ModelEntry {
                id: "moonshine-tiny".to_string(),
                label: "Moonshine Tiny (~36 MB — ultra-light, edge-optimized)".to_string(),
                engine: SttEngine::Moonshine,
                hf_repo: Some("onnx-community/moonshine-tiny-ONNX".to_string()),
            },
            ModelEntry {
                id: "moonshine-base".to_string(),
                label: "Moonshine Base (~90 MB — light, good accuracy)".to_string(),
                engine: SttEngine::Moonshine,
                hf_repo: Some("onnx-community/moonshine-base-ONNX".to_string()),
            },
        ],
        SttEngine::SenseVoice => vec![ModelEntry {
            id: "sensevoice-small".to_string(),
            label: "SenseVoice Small (~240 MB — multi-language, emotion detection)".to_string(),
            engine: SttEngine::SenseVoice,
            hf_repo: Some(
                "csukuangfj/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17".to_string(),
            ),
        }],
    }
}

/// Look up a model entry by its ID across all engines.
pub fn find_model_entry(model_id: &str) -> Option<ModelEntry> {
    for engine in &[
        SttEngine::Whisper,
        SttEngine::Parakeet,
        SttEngine::Moonshine,
        SttEngine::SenseVoice,
    ] {
        for entry in get_engine_models(*engine) {
            if entry.id == model_id {
                return Some(entry);
            }
        }
    }
    None
}

/// Override the directory where Whisper models are stored.
///
/// The directory must exist or be creatable. Returns an error if the path
/// is not a valid directory and cannot be created.
pub fn set_models_dir(path: PathBuf) -> Result<(), String> {
    fs::create_dir_all(&path).map_err(|e| format!("Failed to create models directory: {}", e))?;
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

/// Returns the expected filesystem path for a Whisper GGML model.
pub fn get_model_path(model_name: &str) -> PathBuf {
    get_models_dir().join(format!("ggml-{}.bin", model_name))
}

/// Returns the expected filesystem path for an ONNX model directory.
pub fn get_onnx_model_path(model_id: &str) -> PathBuf {
    get_models_dir().join("onnx").join(model_id)
}

/// Returns the path for a model, choosing the right format based on engine.
pub fn get_engine_model_path(engine: SttEngine, model_id: &str) -> PathBuf {
    match engine {
        SttEngine::Whisper => get_model_path(model_id),
        _ => get_onnx_model_path(model_id),
    }
}

/// Checks whether the model file/directory already exists on disk.
pub fn model_exists(model_name: &str) -> bool {
    get_model_path(model_name).exists()
}

/// Checks whether an ONNX model directory exists on disk.
pub fn onnx_model_exists(model_id: &str) -> bool {
    get_onnx_model_path(model_id).is_dir()
}

/// Checks whether a model exists for the given engine.
pub fn engine_model_exists(engine: SttEngine, model_id: &str) -> bool {
    match engine {
        SttEngine::Whisper => model_exists(model_id),
        _ => onnx_model_exists(model_id),
    }
}

/// Returns the Hugging Face download URL for the given model name.
///
/// Most models follow the `ggerganov/whisper.cpp` naming convention, but
/// some (e.g. distil-whisper variants) are hosted in separate repositories
/// with different filenames.
pub fn get_download_url(model_name: &str) -> String {
    match model_name {
        "distil-large-v3.5" => {
            "https://huggingface.co/distil-whisper/distil-large-v3.5-ggml/resolve/main/ggml-model.bin".to_string()
        }
        _ => format!("{}/ggml-{}.bin", HF_BASE_URL, model_name),
    }
}

/// Returns detailed information about a Whisper model.
///
/// Convenience wrapper around [`get_engine_model_info`] for the Whisper engine.
#[allow(dead_code)]
pub fn get_model_info(model_name: &str) -> ModelInfo {
    get_engine_model_info(SttEngine::Whisper, model_name)
}

/// Returns detailed information about a model for a specific engine.
pub fn get_engine_model_info(engine: SttEngine, model_id: &str) -> ModelInfo {
    let path = get_engine_model_path(engine, model_id);
    let downloaded = match engine {
        SttEngine::Whisper => path.exists(),
        _ => path.is_dir(),
    };
    let size_bytes = if downloaded {
        match engine {
            SttEngine::Whisper => fs::metadata(&path).map(|m| m.len()).unwrap_or(0),
            _ => dir_size(&path),
        }
    } else {
        0
    };

    ModelInfo {
        name: model_id.to_string(),
        path: path.to_string_lossy().to_string(),
        downloaded,
        size_bytes,
        models_dir: get_models_dir().to_string_lossy().to_string(),
        engine: engine.to_string(),
    }
}

/// Recursively compute the total size of a directory in bytes.
fn dir_size(path: &PathBuf) -> u64 {
    let mut total = 0u64;
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            let meta = entry.metadata();
            if let Ok(m) = meta {
                if m.is_file() {
                    total += m.len();
                } else if m.is_dir() {
                    total += dir_size(&entry.path());
                }
            }
        }
    }
    total
}

/// Delete the model file for the given model name.
///
/// Returns an error if the file does not exist or cannot be removed.
pub fn delete_model(model_name: &str) -> Result<(), String> {
    let path = get_model_path(model_name);
    if !path.exists() {
        return Err(format!("Model file does not exist: {}", path.display()));
    }
    fs::remove_file(&path).map_err(|e| format!("Failed to delete model file: {}", e))?;
    Ok(())
}

/// Downloads the model file from Hugging Face to the local models directory.
///
/// Creates the models directory if it does not exist. Streams the download
/// in chunks, emitting `download-progress` events via the provided
/// [`tauri::AppHandle`] so the frontend can display a progress bar.
pub fn download_model_file(model_name: &str, app: &tauri::AppHandle) -> Result<PathBuf, String> {
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

    let response =
        reqwest::blocking::get(&url).map_err(|e| format!("Download request failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!(
            "Download failed with HTTP status {}",
            response.status()
        ));
    }

    let total_bytes = response.content_length().unwrap_or(0);
    let mut bytes_downloaded: u64 = 0;
    let mut last_emitted = std::time::Instant::now();

    let mut file =
        fs::File::create(&path).map_err(|e| format!("Failed to create model file: {}", e))?;

    // Emit initial progress (0%)
    let _ = app.emit(
        "download-progress",
        DownloadProgress {
            bytes_downloaded: 0,
            total_bytes,
        },
    );

    let mut reader = std::io::BufReader::new(response);
    let mut buf = [0u8; 128 * 1024]; // 128 KiB chunks
    loop {
        let n = std::io::Read::read(&mut reader, &mut buf)
            .map_err(|e| format!("Failed to read response body: {}", e))?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])
            .map_err(|e| format!("Failed to write model file: {}", e))?;
        bytes_downloaded += n as u64;

        // Throttle progress events to avoid flooding the frontend
        let now = std::time::Instant::now();
        if now.duration_since(last_emitted).as_millis() >= 100 {
            last_emitted = now;
            let _ = app.emit(
                "download-progress",
                DownloadProgress {
                    bytes_downloaded,
                    total_bytes,
                },
            );
        }
    }

    // Emit final 100% progress
    let _ = app.emit(
        "download-progress",
        DownloadProgress {
            bytes_downloaded,
            total_bytes,
        },
    );

    eprintln!("Model saved to {}", path.display());
    Ok(path)
}

/// Returns the list of HuggingFace file paths to download for a given
/// ONNX model. Each entry is `(hf_file_path, local_filename)`.
fn onnx_model_files(engine: SttEngine) -> Vec<(&'static str, &'static str)> {
    match engine {
        SttEngine::Moonshine => vec![
            ("onnx/encoder_model.onnx", "encoder_model.onnx"),
            (
                "onnx/decoder_model_merged.onnx",
                "decoder_model_merged.onnx",
            ),
            ("tokenizer.json", "tokenizer.json"),
        ],
        SttEngine::Parakeet => vec![
            ("encoder-model.int8.onnx", "encoder-model.int8.onnx"),
            (
                "decoder_joint-model.int8.onnx",
                "decoder_joint-model.int8.onnx",
            ),
            ("nemo128.onnx", "nemo128.onnx"),
            ("vocab.txt", "vocab.txt"),
        ],
        SttEngine::SenseVoice => vec![
            ("model.int8.onnx", "model.int8.onnx"),
            ("tokens.txt", "tokens.txt"),
        ],
        SttEngine::Whisper => vec![],
    }
}

/// Downloads an ONNX model from HuggingFace.
///
/// Each model's required ONNX files are downloaded individually from the
/// HuggingFace repository into the local ONNX models subdirectory.
/// Progress events are emitted via `download-progress` so the frontend
/// can display a progress bar.
pub fn download_onnx_model(
    engine: SttEngine,
    model_id: &str,
    app: &tauri::AppHandle,
) -> Result<PathBuf, String> {
    let model_dir = get_onnx_model_path(model_id);

    if model_dir.is_dir() {
        return Ok(model_dir);
    }

    let entry = find_model_entry(model_id).ok_or_else(|| format!("Unknown model: {}", model_id))?;

    let hf_repo = entry
        .hf_repo
        .ok_or_else(|| format!("Model {} has no HuggingFace repo configured", model_id))?;

    if engine == SttEngine::Whisper {
        return Err("Use download_model_file for Whisper models".to_string());
    }

    let files = onnx_model_files(engine);
    if files.is_empty() {
        return Err(format!("No ONNX files configured for model {}", model_id));
    }

    fs::create_dir_all(&model_dir)
        .map_err(|e| format!("Failed to create ONNX model directory: {}", e))?;

    eprintln!("Downloading ONNX model {} from {} ...", model_id, hf_repo);

    let _ = app.emit(
        "download-progress",
        DownloadProgress {
            bytes_downloaded: 0,
            total_bytes: 0,
        },
    );

    let mut total_downloaded: u64 = 0;

    for (hf_path, local_name) in &files {
        let url = format!(
            "https://huggingface.co/{}/resolve/main/{}",
            hf_repo, hf_path
        );
        let dest = model_dir.join(local_name);

        eprintln!("  Downloading {} ...", local_name);

        let response = reqwest::blocking::get(&url)
            .map_err(|e| format!("Download request failed for {}: {}", local_name, e))?;

        if !response.status().is_success() {
            // Clean up partial directory on failure
            let _ = fs::remove_dir_all(&model_dir);
            return Err(format!(
                "Download failed for {} with HTTP status {}",
                local_name,
                response.status()
            ));
        }

        let total_bytes = response.content_length().unwrap_or(0);
        let mut file_downloaded: u64 = 0;
        let mut last_emitted = std::time::Instant::now();

        let mut file = fs::File::create(&dest)
            .map_err(|e| format!("Failed to create file {}: {}", local_name, e))?;

        let mut reader = std::io::BufReader::new(response);
        let mut buf = [0u8; 128 * 1024];
        loop {
            let n = std::io::Read::read(&mut reader, &mut buf)
                .map_err(|e| format!("Failed to read response for {}: {}", local_name, e))?;
            if n == 0 {
                break;
            }
            file.write_all(&buf[..n])
                .map_err(|e| format!("Failed to write file {}: {}", local_name, e))?;
            file_downloaded += n as u64;

            let now = std::time::Instant::now();
            if now.duration_since(last_emitted).as_millis() >= 100 {
                last_emitted = now;
                let _ = app.emit(
                    "download-progress",
                    DownloadProgress {
                        bytes_downloaded: total_downloaded + file_downloaded,
                        total_bytes: total_downloaded + total_bytes,
                    },
                );
            }
        }

        total_downloaded += file_downloaded;
    }

    // Emit completion
    let _ = app.emit(
        "download-progress",
        DownloadProgress {
            bytes_downloaded: total_downloaded,
            total_bytes: total_downloaded,
        },
    );

    eprintln!("ONNX model saved to {}", model_dir.display());
    Ok(model_dir)
}

/// Delete a model for any engine.
pub fn delete_engine_model(engine: SttEngine, model_id: &str) -> Result<(), String> {
    match engine {
        SttEngine::Whisper => delete_model(model_id),
        _ => {
            let path = get_onnx_model_path(model_id);
            if !path.is_dir() {
                return Err(format!(
                    "ONNX model directory does not exist: {}",
                    path.display()
                ));
            }
            fs::remove_dir_all(&path)
                .map_err(|e| format!("Failed to delete ONNX model directory: {}", e))?;
            Ok(())
        }
    }
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
        let _guard = lock_with_temp_dir(tmp.path());
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

    #[test]
    fn get_engine_model_path_whisper_uses_ggml_format() {
        // Arrange
        let tmp = tempfile::tempdir().unwrap();
        let _guard = lock_with_temp_dir(tmp.path());

        // Act
        let path = get_engine_model_path(SttEngine::Whisper, "base");

        // Assert
        assert_eq!(path.file_name().unwrap(), "ggml-base.bin");

        // Cleanup
        clear_custom_dir();
    }

    #[test]
    fn get_engine_model_path_onnx_uses_subdirectory() {
        // Arrange
        let tmp = tempfile::tempdir().unwrap();
        let _guard = lock_with_temp_dir(tmp.path());

        // Act
        let path = get_engine_model_path(SttEngine::Parakeet, "parakeet-tdt-0.6b");

        // Assert
        assert!(path.ends_with("onnx/parakeet-tdt-0.6b"));

        // Cleanup
        clear_custom_dir();
    }

    #[test]
    fn engine_model_exists_checks_directory_for_onnx() {
        // Arrange
        let tmp = tempfile::tempdir().unwrap();
        let _guard = lock_with_temp_dir(tmp.path());
        let onnx_dir = tmp.path().join("onnx").join("moonshine-tiny");
        fs::create_dir_all(&onnx_dir).unwrap();

        // Act
        let exists = engine_model_exists(SttEngine::Moonshine, "moonshine-tiny");

        // Assert
        assert!(exists);

        // Cleanup
        clear_custom_dir();
    }

    #[test]
    fn get_engine_models_returns_models_for_each_engine() {
        // Arrange / Act / Assert
        let whisper = get_engine_models(SttEngine::Whisper);
        assert!(whisper.len() >= 5);
        assert!(whisper.iter().any(|m| m.id == "base"));
        assert!(whisper.iter().any(|m| m.id == "distil-large-v3.5"));

        let parakeet = get_engine_models(SttEngine::Parakeet);
        assert!(!parakeet.is_empty());
        assert!(parakeet[0].hf_repo.is_some());

        let moonshine = get_engine_models(SttEngine::Moonshine);
        assert!(moonshine.len() >= 2);

        let sensevoice = get_engine_models(SttEngine::SenseVoice);
        assert!(!sensevoice.is_empty());
    }

    #[test]
    fn find_model_entry_finds_whisper_model() {
        // Arrange / Act
        let entry = find_model_entry("base");

        // Assert
        assert!(entry.is_some());
        let entry = entry.unwrap();
        assert_eq!(entry.engine, SttEngine::Whisper);
        assert!(entry.hf_repo.is_none());
    }

    #[test]
    fn find_model_entry_finds_onnx_model() {
        // Arrange / Act
        let entry = find_model_entry("parakeet-tdt-0.6b");

        // Assert
        assert!(entry.is_some());
        let entry = entry.unwrap();
        assert_eq!(entry.engine, SttEngine::Parakeet);
        assert!(entry.hf_repo.is_some());
    }

    #[test]
    fn find_model_entry_returns_none_for_unknown() {
        // Arrange / Act
        let entry = find_model_entry("nonexistent-model");

        // Assert
        assert!(entry.is_none());
    }

    #[test]
    fn delete_engine_model_removes_onnx_directory() {
        // Arrange
        let tmp = tempfile::tempdir().unwrap();
        let _guard = lock_with_temp_dir(tmp.path());
        let onnx_dir = tmp.path().join("onnx").join("moonshine-tiny");
        fs::create_dir_all(&onnx_dir).unwrap();
        fs::write(onnx_dir.join("model.onnx"), b"fake").unwrap();

        // Act
        let result = delete_engine_model(SttEngine::Moonshine, "moonshine-tiny");

        // Assert
        assert!(result.is_ok());
        assert!(!onnx_dir.exists());

        // Cleanup
        clear_custom_dir();
    }

    #[test]
    fn get_engine_model_info_onnx_shows_directory_size() {
        // Arrange
        let tmp = tempfile::tempdir().unwrap();
        let _guard = lock_with_temp_dir(tmp.path());
        let onnx_dir = tmp.path().join("onnx").join("moonshine-tiny");
        fs::create_dir_all(&onnx_dir).unwrap();
        fs::write(onnx_dir.join("model.onnx"), vec![0u8; 2048]).unwrap();
        fs::write(onnx_dir.join("vocab.txt"), vec![0u8; 512]).unwrap();

        // Act
        let info = get_engine_model_info(SttEngine::Moonshine, "moonshine-tiny");

        // Assert
        assert!(info.downloaded);
        assert_eq!(info.size_bytes, 2560);
        assert_eq!(info.engine, "moonshine");

        // Cleanup
        clear_custom_dir();
    }

    #[test]
    fn get_download_url_uses_custom_url_for_distil() {
        // Arrange
        let model_name = "distil-large-v3.5";

        // Act
        let url = get_download_url(model_name);

        // Assert
        assert_eq!(
            url,
            "https://huggingface.co/distil-whisper/distil-large-v3.5-ggml/resolve/main/ggml-model.bin"
        );
    }
}
