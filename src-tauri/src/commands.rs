//! Tauri command handlers for audio capture and transcription management.

use cpal::Stream;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use tauri::ipc::Channel;
use tauri::{Emitter, State};

use crate::audio::capture;
use crate::audio::pipeline;
use crate::audio::system_capture::{self, SystemCaptureHandle};
use crate::audio::types::{AudioDevice, AudioLevels, CaptureError, TranscriptSegment};
use crate::markdown;
use crate::settings::{self, Settings};
use crate::transcription::manager::{self, ModelInfo};
use crate::transcription::worker::TranscriptionWorker;

/// Application state managed by Tauri.
pub struct AppState {
    pub is_recording: bool,
    /// Active mic device ID during recording.
    pub mic_device_id: Option<String>,
    /// Active system device ID during recording.
    pub system_device_id: Option<String>,
    /// Preferred mic device ID (persisted across launches).
    pub preferred_mic_device_id: Option<String>,
    /// Preferred system audio device ID (persisted across launches).
    pub preferred_system_device_id: Option<String>,
    pub mic_rms: Arc<AtomicU32>,
    pub system_rms: Arc<AtomicU32>,
    pub mic_gain: Arc<AtomicU32>,
    pub vad_threshold: Arc<AtomicU32>,
    pub mic_stream: Option<Stream>,
    pub system_capture: Option<SystemCaptureHandle>,
    pub worker: Option<TranscriptionWorker>,
    pub mic_sample_rate: u32,
    pub mic_channels: u16,
    pub system_sample_rate: u32,
    pub system_channels: u16,
    /// App config directory for persisting settings.
    pub config_dir: PathBuf,
    /// Custom output directory for transcript files (None = ~/EchoNotes/).
    pub output_dir: Option<String>,
    /// Auto-stop silence timeout in seconds (None = disabled).
    pub silence_timeout_seconds: Option<u32>,
    /// Timestamp when the current recording started.
    pub recording_start_time: Option<chrono::DateTime<chrono::Local>>,
    /// Whether the recording is currently paused.
    pub is_paused: bool,
    /// Path to the transcript file for the current recording session.
    pub current_transcript_path: Option<PathBuf>,
    /// Meeting name for the current recording session.
    pub current_meeting_name: Option<String>,
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
            preferred_mic_device_id: None,
            preferred_system_device_id: None,
            mic_rms: Arc::new(AtomicU32::new(0)),
            system_rms: Arc::new(AtomicU32::new(0)),
            mic_gain: Arc::new(AtomicU32::new(1.0f32.to_bits())),
            vad_threshold: Arc::new(AtomicU32::new(
                pipeline::DEFAULT_VAD_THRESHOLD.to_bits(),
            )),
            mic_stream: None,
            system_capture: None,
            worker: None,
            mic_sample_rate: 0,
            mic_channels: 0,
            system_sample_rate: 0,
            system_channels: 0,
            config_dir: PathBuf::new(),
            output_dir: None,
            silence_timeout_seconds: Some(300),
            recording_start_time: None,
            is_paused: false,
            current_transcript_path: None,
            current_meeting_name: None,
        }
    }
}

/// Build a `Settings` snapshot from the current app state and persist it.
fn save_current_settings(app_state: &AppState) {
    let settings = Settings {
        mic_device_id: app_state.preferred_mic_device_id.clone(),
        system_device_id: app_state.preferred_system_device_id.clone(),
        mic_gain: f32::from_bits(app_state.mic_gain.load(Ordering::Relaxed)),
        vad_threshold: f32::from_bits(app_state.vad_threshold.load(Ordering::Relaxed)),
        models_dir: manager::get_custom_models_dir()
            .map(|p| p.to_string_lossy().to_string()),
        output_dir: app_state.output_dir.clone(),
        silence_timeout_seconds: app_state.silence_timeout_seconds,
    };
    if let Err(e) = settings::save(&app_state.config_dir, &settings) {
        eprintln!("Failed to save settings: {}", e);
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

/// Retrieve persisted application settings.
#[tauri::command]
pub fn get_settings(state: State<'_, Mutex<AppState>>) -> Result<Settings, String> {
    let app_state = state.lock().map_err(|e| format!("Lock error: {}", e))?;
    Ok(settings::load(&app_state.config_dir))
}

/// Set and persist the preferred microphone device ID.
#[tauri::command]
pub fn set_preferred_mic(device_id: String, state: State<'_, Mutex<AppState>>) -> Result<(), String> {
    let mut app_state = state.lock().map_err(|e| format!("Lock error: {}", e))?;
    app_state.preferred_mic_device_id = Some(device_id);
    save_current_settings(&app_state);
    Ok(())
}

/// Set and persist the preferred system audio device ID.
#[tauri::command]
pub fn set_preferred_system_device(device_id: String, state: State<'_, Mutex<AppState>>) -> Result<(), String> {
    let mut app_state = state.lock().map_err(|e| format!("Lock error: {}", e))?;
    app_state.preferred_system_device_id = Some(device_id);
    save_current_settings(&app_state);
    Ok(())
}

/// Set and persist the output directory for transcript files.
///
/// Pass an empty string to reset to the default (`~/EchoNotes/`).
#[tauri::command]
pub fn set_output_dir(path: String, state: State<'_, Mutex<AppState>>) -> Result<(), String> {
    let mut app_state = state.lock().map_err(|e| format!("Lock error: {}", e))?;
    app_state.output_dir = if path.is_empty() { None } else { Some(path) };
    save_current_settings(&app_state);
    Ok(())
}

/// Get the resolved output directory path.
#[tauri::command]
pub fn get_output_dir(state: State<'_, Mutex<AppState>>) -> Result<String, String> {
    let app_state = state.lock().map_err(|e| format!("Lock error: {}", e))?;
    let loaded = settings::load(&app_state.config_dir);
    Ok(loaded.output_dir_resolved().to_string_lossy().to_string())
}

/// Set and persist the auto-stop silence timeout.
///
/// Pass 0 to disable auto-stop.
#[tauri::command]
pub fn set_silence_timeout(seconds: u32, state: State<'_, Mutex<AppState>>) -> Result<(), String> {
    let mut app_state = state.lock().map_err(|e| format!("Lock error: {}", e))?;
    app_state.silence_timeout_seconds = if seconds == 0 { None } else { Some(seconds) };
    save_current_settings(&app_state);
    Ok(())
}

/// Result returned by `start_recording`.
#[derive(serde::Serialize)]
pub struct StartRecordingResult {
    /// Path to the newly created transcript file.
    pub transcript_path: String,
    /// Meeting name used for this session.
    pub meeting_name: String,
}

/// Start recording from the specified mic and system audio devices.
///
/// Launches audio capture streams and a background transcription worker
/// that sends `TranscriptSegment`s to the frontend via the provided channel.
/// Creates the transcript file immediately so it appears in the meetings list.
#[tauri::command]
pub fn start_recording(
    mic_device_id: String,
    system_device_id: String,
    on_transcript: Channel<TranscriptSegment>,
    state: State<'_, Mutex<AppState>>,
    app: tauri::AppHandle,
) -> Result<StartRecordingResult, String> {
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

    // Start mic capture with current gain setting
    let mic_handle =
        capture::start_capture(&mic_device_id, Arc::clone(&app_state.mic_gain))
            .map_err(|e| e.to_string())?;
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
        Arc::clone(&app_state.vad_threshold),
        app_state.silence_timeout_seconds,
        on_transcript,
    )?;

    let start_time = chrono::Local::now();
    let loaded = settings::load(&app_state.config_dir);
    let output_dir = loaded.output_dir_resolved();
    let meeting_name = markdown::resolve_meeting_name(&start_time);

    // Create the transcript file immediately with just the header
    let transcript_path = markdown::write_transcript(
        &output_dir,
        &[],
        &meeting_name,
        start_time,
        start_time,
    )?;

    app_state.mic_stream = Some(mic_handle.stream);
    app_state.system_capture = Some(system_handle);
    app_state.mic_device_id = Some(mic_device_id);
    app_state.system_device_id = Some(system_device_id);
    app_state.worker = Some(worker);
    app_state.is_recording = true;
    app_state.is_paused = false;
    app_state.recording_start_time = Some(start_time);
    app_state.current_transcript_path = Some(transcript_path.clone());
    app_state.current_meeting_name = Some(meeting_name.clone());

    // Notify frontend that meetings list changed
    let _ = app.emit("meetings-changed", ());

    Ok(StartRecordingResult {
        transcript_path: transcript_path.to_string_lossy().to_string(),
        meeting_name,
    })
}

/// Result returned by `stop_recording`.
#[derive(serde::Serialize)]
pub struct StopRecordingResult {
    /// Path to the saved transcript file, if any segments were recorded.
    pub transcript_path: Option<String>,
    /// Number of transcript segments in the recording.
    pub segment_count: usize,
}

/// Stop the current recording session.
///
/// Drops audio streams, stops the transcription worker, writes the final
/// transcript to the file created at start, and resets state.
#[tauri::command]
pub fn stop_recording(
    state: State<'_, Mutex<AppState>>,
    app: tauri::AppHandle,
) -> Result<StopRecordingResult, String> {
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
    app_state.is_paused = false;

    // Stop the transcription worker and collect accumulated segments
    let segments = if let Some(ref mut worker) = app_state.worker {
        worker.stop()
    } else {
        Vec::new()
    };
    app_state.worker = None;

    // Reset RMS levels
    app_state.mic_rms.store(0u32, Ordering::Relaxed);
    app_state.system_rms.store(0u32, Ordering::Relaxed);

    // Write final transcript to the file created at start
    let segment_count = segments.len();
    let transcript_path = app_state.current_transcript_path.take();
    let meeting_name = app_state.current_meeting_name.take();
    let start_time = app_state
        .recording_start_time
        .take()
        .unwrap_or_else(chrono::Local::now);

    let path_str = if let Some(ref path) = transcript_path {
        let end_time = chrono::Local::now();
        let name = meeting_name.as_deref().unwrap_or("Meeting");
        if let Err(e) = markdown::update_transcript(path, &segments, name, start_time, end_time) {
            eprintln!("Failed to write final transcript: {}", e);
        }
        Some(path.to_string_lossy().to_string())
    } else {
        None
    };

    // Notify frontend that meetings list changed
    let _ = app.emit("meetings-changed", ());

    Ok(StopRecordingResult {
        transcript_path: path_str,
        segment_count,
    })
}

/// Get current audio input levels for the mic and system streams.
#[tauri::command]
pub fn get_audio_levels(state: State<'_, Mutex<AppState>>) -> Result<AudioLevels, String> {
    let app_state = state.lock().map_err(|e| format!("Lock error: {}", e))?;

    let mic_rms = f32::from_bits(app_state.mic_rms.load(Ordering::Relaxed));
    let system_rms = f32::from_bits(app_state.system_rms.load(Ordering::Relaxed));

    let auto_stopped = app_state
        .worker
        .as_ref()
        .map(|w| w.auto_stopped())
        .unwrap_or(false);

    Ok(AudioLevels {
        mic_rms,
        system_rms,
        is_paused: app_state.is_paused,
        auto_stopped,
    })
}

/// Set the microphone gain multiplier (1.0 = unity, 2.0 = double, etc.).
///
/// Takes effect immediately — the gain is applied in the audio capture callback,
/// so it affects both the level meter and the audio sent to Whisper.
/// The new value is automatically persisted to disk.
#[tauri::command]
pub fn set_mic_gain(gain: f32, state: State<'_, Mutex<AppState>>) -> Result<(), String> {
    let app_state = state.lock().map_err(|e| format!("Lock error: {}", e))?;
    let clamped = gain.clamp(0.1, 10.0);
    app_state.mic_gain.store(clamped.to_bits(), Ordering::Relaxed);
    save_current_settings(&app_state);
    Ok(())
}

/// Get the current microphone gain multiplier.
#[tauri::command]
pub fn get_mic_gain(state: State<'_, Mutex<AppState>>) -> Result<f32, String> {
    let app_state = state.lock().map_err(|e| format!("Lock error: {}", e))?;
    Ok(f32::from_bits(app_state.mic_gain.load(Ordering::Relaxed)))
}

/// Set the VAD (voice activity detection) sensitivity threshold.
///
/// Lower values detect quieter speech; higher values require louder input.
/// Takes effect immediately during recording.
/// The new value is automatically persisted to disk.
#[tauri::command]
pub fn set_vad_threshold(threshold: f32, state: State<'_, Mutex<AppState>>) -> Result<(), String> {
    let app_state = state.lock().map_err(|e| format!("Lock error: {}", e))?;
    let clamped = threshold.clamp(0.0005, 0.1);
    app_state
        .vad_threshold
        .store(clamped.to_bits(), Ordering::Relaxed);
    save_current_settings(&app_state);
    Ok(())
}

/// Get the current VAD sensitivity threshold.
#[tauri::command]
pub fn get_vad_threshold(state: State<'_, Mutex<AppState>>) -> Result<f32, String> {
    let app_state = state.lock().map_err(|e| format!("Lock error: {}", e))?;
    Ok(f32::from_bits(
        app_state.vad_threshold.load(Ordering::Relaxed),
    ))
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
/// The new value is automatically persisted to disk.
#[tauri::command]
pub fn set_models_dir(path: String, state: State<'_, Mutex<AppState>>) -> Result<(), String> {
    if path.is_empty() {
        manager::reset_models_dir();
    } else {
        manager::set_models_dir(std::path::PathBuf::from(path))?;
    }
    let app_state = state.lock().map_err(|e| format!("Lock error: {}", e))?;
    save_current_settings(&app_state);
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

/// List meetings in the output directory.
#[tauri::command]
pub fn list_meetings(state: State<'_, Mutex<AppState>>) -> Result<Vec<markdown::MeetingEntry>, String> {
    let app_state = state.lock().map_err(|e| format!("Lock error: {}", e))?;
    let loaded = settings::load(&app_state.config_dir);
    let output_dir = loaded.output_dir_resolved();
    Ok(markdown::list_meetings_in_dir(&output_dir))
}

/// Read the contents of a transcript file.
#[tauri::command]
pub fn read_meeting_transcript(path: String) -> Result<String, String> {
    std::fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read transcript: {}", e))
}

/// Pause the current recording session. Audio streams continue but samples
/// are discarded.
#[tauri::command]
pub fn pause_recording(state: State<'_, Mutex<AppState>>) -> Result<(), String> {
    let mut app_state = state.lock().map_err(|e| format!("Lock error: {}", e))?;
    if !app_state.is_recording {
        return Err(CaptureError::NotRecording.to_string());
    }
    if let Some(ref worker) = app_state.worker {
        worker.pause();
    }
    app_state.is_paused = true;
    Ok(())
}

/// Resume a paused recording session.
#[tauri::command]
pub fn resume_recording(state: State<'_, Mutex<AppState>>) -> Result<(), String> {
    let mut app_state = state.lock().map_err(|e| format!("Lock error: {}", e))?;
    if !app_state.is_recording {
        return Err(CaptureError::NotRecording.to_string());
    }
    if let Some(ref worker) = app_state.worker {
        worker.resume();
    }
    app_state.is_paused = false;
    Ok(())
}

/// Save the current transcript to disk (periodic save during recording).
///
/// Reads segments from the worker's shared accumulator and rewrites the
/// transcript file that was created at recording start.
#[tauri::command]
pub fn save_current_transcript(
    state: State<'_, Mutex<AppState>>,
    app: tauri::AppHandle,
) -> Result<usize, String> {
    let app_state = state.lock().map_err(|e| format!("Lock error: {}", e))?;

    if !app_state.is_recording {
        return Err(CaptureError::NotRecording.to_string());
    }

    let path = app_state
        .current_transcript_path
        .as_ref()
        .ok_or("No transcript path")?;
    let name = app_state
        .current_meeting_name
        .as_deref()
        .unwrap_or("Meeting");
    let start_time = app_state
        .recording_start_time
        .unwrap_or_else(chrono::Local::now);

    let segments = app_state
        .worker
        .as_ref()
        .map(|w| w.get_segments())
        .unwrap_or_default();

    let count = segments.len();
    let end_time = chrono::Local::now();

    markdown::update_transcript(path, &segments, name, start_time, end_time)?;

    // Notify frontend that meetings list changed
    let _ = app.emit("meetings-changed", ());

    Ok(count)
}
