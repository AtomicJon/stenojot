//! Tauri command handlers for audio capture and transcription management.

use cpal::Stream;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use tauri::ipc::Channel;
use tauri::State;

use crate::audio::capture;
use crate::audio::system_capture::{self, SystemCaptureHandle};
use crate::audio::types::{AudioDevice, AudioLevels, CaptureError, TranscriptSegment};
use crate::transcription::manager::{self, ModelInfo};
use crate::transcription::worker::TranscriptionWorker;

/// Application state managed by Tauri.
pub struct AppState {
    pub is_recording: bool,
    pub mic_device_id: Option<String>,
    pub system_device_id: Option<String>,
    pub mic_rms: Arc<AtomicU32>,
    pub system_rms: Arc<AtomicU32>,
    pub mic_stream: Option<Stream>,
    pub system_capture: Option<SystemCaptureHandle>,
    pub worker: Option<TranscriptionWorker>,
    pub mic_sample_rate: u32,
    pub mic_channels: u16,
    pub system_sample_rate: u32,
    pub system_channels: u16,
}

// Safety: cpal::Stream is !Send and !Sync, but we only ever access it
// while holding the Mutex lock from a single thread at a time.
// The streams themselves are audio device handles that are safe to drop
// from any thread.
unsafe impl Send for AppState {}
unsafe impl Sync for AppState {}

impl AppState {
    /// Create a new default application state.
    pub fn new() -> Self {
        Self {
            is_recording: false,
            mic_device_id: None,
            system_device_id: None,
            mic_rms: Arc::new(AtomicU32::new(0)),
            system_rms: Arc::new(AtomicU32::new(0)),
            mic_stream: None,
            system_capture: None,
            worker: None,
            mic_sample_rate: 0,
            mic_channels: 0,
            system_sample_rate: 0,
            system_channels: 0,
        }
    }
}

/// List available microphone input devices (via cpal/ALSA).
#[tauri::command]
pub fn get_audio_devices() -> Result<Vec<AudioDevice>, String> {
    capture::list_input_devices().map_err(|e| e.to_string())
}

/// List available system audio monitor sources (via PulseAudio/PipeWire).
///
/// Monitor sources capture system audio output — what comes through speakers
/// or headphones. These are not visible through ALSA, so we query PulseAudio
/// directly.
#[tauri::command]
pub fn get_system_audio_devices() -> Result<Vec<AudioDevice>, String> {
    system_capture::list_monitor_sources().map_err(|e| e.to_string())
}

/// Start recording from the specified mic and system audio devices.
///
/// Launches audio capture streams and a background transcription worker
/// that sends `TranscriptSegment`s to the frontend via the provided channel.
#[tauri::command]
pub fn start_recording(
    mic_device_id: String,
    system_device_id: String,
    on_transcript: Channel<TranscriptSegment>,
    state: State<'_, Mutex<AppState>>,
) -> Result<(), String> {
    let mut app_state = state.lock().map_err(|e| format!("Lock error: {}", e))?;

    if app_state.is_recording {
        return Err(CaptureError::AlreadyRecording.to_string());
    }

    // Ensure the Whisper model is available before starting
    let model_name = "base";
    if !manager::model_exists(model_name) {
        return Err("Whisper model not downloaded. Call download_model first.".to_string());
    }
    let model_path = manager::get_model_path(model_name);

    // Start mic capture
    let mic_handle = capture::start_capture(&mic_device_id).map_err(|e| e.to_string())?;
    app_state.mic_rms = mic_handle.rms_level;
    app_state.mic_sample_rate = mic_handle.sample_rate;
    app_state.mic_channels = mic_handle.channels;

    // Start system audio capture via PulseAudio monitor source
    let mut system_handle =
        system_capture::start_system_capture(&system_device_id).map_err(|e| e.to_string())?;
    app_state.system_rms = Arc::clone(&system_handle.rms_level);
    app_state.system_sample_rate = system_handle.sample_rate;
    app_state.system_channels = system_handle.channels;

    // Take the consumer out of the system handle — worker needs ownership
    let system_consumer = system_handle
        .consumer
        .take()
        .ok_or("System audio consumer already taken")?;

    // Spawn the transcription worker with ownership of the ring buffer consumers
    let worker = TranscriptionWorker::start(
        model_path,
        mic_handle.consumer,
        system_consumer,
        app_state.mic_sample_rate,
        app_state.mic_channels,
        app_state.system_sample_rate,
        app_state.system_channels,
        on_transcript,
    )?;

    app_state.mic_stream = Some(mic_handle.stream);
    app_state.system_capture = Some(system_handle);
    app_state.mic_device_id = Some(mic_device_id);
    app_state.system_device_id = Some(system_device_id);
    app_state.worker = Some(worker);
    app_state.is_recording = true;

    Ok(())
}

/// Stop the current recording session.
///
/// Drops audio streams, stops the transcription worker, and resets state.
#[tauri::command]
pub fn stop_recording(state: State<'_, Mutex<AppState>>) -> Result<(), String> {
    let mut app_state = state.lock().map_err(|e| format!("Lock error: {}", e))?;

    if !app_state.is_recording {
        return Err(CaptureError::NotRecording.to_string());
    }

    // Stop audio capture
    app_state.mic_stream = None;
    if let Some(ref mut sys) = app_state.system_capture {
        sys.stop();
    }
    app_state.system_capture = None;
    app_state.mic_device_id = None;
    app_state.system_device_id = None;
    app_state.is_recording = false;

    // Stop the transcription worker (flushes remaining audio)
    if let Some(ref mut worker) = app_state.worker {
        worker.stop();
    }
    app_state.worker = None;

    // Reset RMS levels
    app_state.mic_rms.store(0u32, Ordering::Relaxed);
    app_state.system_rms.store(0u32, Ordering::Relaxed);

    Ok(())
}

/// Get current audio input levels for the mic and system streams.
#[tauri::command]
pub fn get_audio_levels(state: State<'_, Mutex<AppState>>) -> Result<AudioLevels, String> {
    let app_state = state.lock().map_err(|e| format!("Lock error: {}", e))?;

    let mic_rms = f32::from_bits(app_state.mic_rms.load(Ordering::Relaxed));
    let system_rms = f32::from_bits(app_state.system_rms.load(Ordering::Relaxed));

    Ok(AudioLevels {
        mic_rms,
        system_rms,
    })
}

/// Get detailed info about the Whisper model (path, size, download status).
#[tauri::command]
pub fn get_model_info() -> Result<ModelInfo, String> {
    Ok(manager::get_model_info("base"))
}

/// Delete the downloaded Whisper model file.
#[tauri::command]
pub fn delete_model() -> Result<(), String> {
    manager::delete_model("base")
}

/// Set a custom directory for storing Whisper model files.
///
/// Pass an empty string to reset to the default location.
#[tauri::command]
pub fn set_models_dir(path: String) -> Result<(), String> {
    if path.is_empty() {
        manager::reset_models_dir();
    } else {
        manager::set_models_dir(std::path::PathBuf::from(path))?;
    }
    Ok(())
}

/// Download the Whisper base model from Hugging Face.
///
/// This is a blocking download (~140 MB). Returns the path to the
/// downloaded model file on success.
#[tauri::command]
pub fn download_model() -> Result<String, String> {
    let path = manager::download_model_file("base")?;
    Ok(path.to_string_lossy().to_string())
}
