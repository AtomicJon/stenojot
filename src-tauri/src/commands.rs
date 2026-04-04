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
use crate::llm;
use crate::llm::provider::{parse_provider, LlmConfig};
use crate::markdown;
use crate::settings::{self, Settings};
use crate::transcription::manager::{self, ModelInfo};
use crate::transcription::worker::{TranscriptionWorker, WorkerConfig};

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
    /// Custom output directory for transcript files (None = ~/StenoJot/).
    pub output_dir: Option<String>,
    /// Auto-stop silence timeout in seconds (None = disabled).
    pub silence_timeout_seconds: Option<u32>,
    /// STT engine identifier ("whisper", "parakeet", "moonshine", "sensevoice").
    pub stt_engine: String,
    /// Whisper model name (e.g. "base", "small", "medium").
    pub whisper_model: String,
    /// Model identifier for the active non-Whisper STT engine.
    pub stt_model: Option<String>,
    /// Initial prompt to guide Whisper transcription.
    pub initial_prompt: Option<String>,
    /// Maximum segment duration in seconds before forced transcription.
    pub max_segment_seconds: u32,
    /// Timestamp when the current recording started.
    pub recording_start_time: Option<chrono::DateTime<chrono::Local>>,
    /// Whether the recording is currently paused.
    pub is_paused: bool,
    /// Path to the transcript file for the current recording session.
    pub current_transcript_path: Option<PathBuf>,
    /// Meeting name for the current recording session.
    pub current_meeting_name: Option<String>,
    /// LLM provider for summary generation ("ollama", "anthropic", "openai").
    pub llm_provider: String,
    /// LLM model name override (None = use provider default).
    pub llm_model: Option<String>,
    /// API key for cloud LLM providers (Anthropic, OpenAI).
    pub llm_api_key: Option<String>,
    /// Custom base URL for LLM provider.
    pub llm_base_url: Option<String>,
    /// Whether to auto-generate summaries after recording stops.
    pub auto_summary: bool,
}

// Safety: cpal::Stream is !Send and !Sync, but we only ever access it
// while holding the Mutex lock from a single thread at a time.
// The streams themselves are audio device handles that are safe to drop
// from any thread.
unsafe impl Send for AppState {}
unsafe impl Sync for AppState {}

impl AppState {
    /// Resolve the active engine and model ID.
    ///
    /// Derives the engine from the model entry when possible so that the
    /// engine and model are always consistent, even if the UI updates
    /// `stt_engine` and `stt_model` independently.
    pub fn resolve_engine_and_model(&self) -> (crate::transcription::engine::SttEngine, String) {
        let engine = crate::transcription::engine::parse_engine(&self.stt_engine);
        let model_id = match engine {
            crate::transcription::engine::SttEngine::Whisper => self.whisper_model.clone(),
            _ => self
                .stt_model
                .clone()
                .unwrap_or_else(|| manager::get_engine_models(engine)[0].id.clone()),
        };
        // Re-derive the engine from the model entry to handle mismatches.
        let engine = manager::find_model_entry(&model_id)
            .map(|e| e.engine)
            .unwrap_or(engine);
        (engine, model_id)
    }

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
            vad_threshold: Arc::new(AtomicU32::new(pipeline::DEFAULT_VAD_THRESHOLD.to_bits())),
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
            stt_engine: settings::DEFAULT_STT_ENGINE.to_string(),
            whisper_model: settings::DEFAULT_WHISPER_MODEL.to_string(),
            stt_model: None,
            initial_prompt: None,
            max_segment_seconds: settings::DEFAULT_MAX_SEGMENT_SECONDS,
            recording_start_time: None,
            is_paused: false,
            current_transcript_path: None,
            current_meeting_name: None,
            llm_provider: settings::DEFAULT_LLM_PROVIDER.to_string(),
            llm_model: None,
            llm_api_key: None,
            llm_base_url: None,
            auto_summary: true,
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
        models_dir: manager::get_custom_models_dir().map(|p| p.to_string_lossy().to_string()),
        output_dir: app_state.output_dir.clone(),
        silence_timeout_seconds: app_state.silence_timeout_seconds,
        stt_engine: app_state.stt_engine.clone(),
        whisper_model: app_state.whisper_model.clone(),
        stt_model: app_state.stt_model.clone(),
        initial_prompt: app_state.initial_prompt.clone(),
        max_segment_seconds: app_state.max_segment_seconds,
        llm_provider: app_state.llm_provider.clone(),
        llm_model: app_state.llm_model.clone(),
        llm_api_key: app_state.llm_api_key.clone(),
        llm_base_url: app_state.llm_base_url.clone(),
        auto_summary: app_state.auto_summary,
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
pub fn set_preferred_mic(
    device_id: String,
    state: State<'_, Mutex<AppState>>,
) -> Result<(), String> {
    let mut app_state = state.lock().map_err(|e| format!("Lock error: {}", e))?;
    app_state.preferred_mic_device_id = Some(device_id);
    save_current_settings(&app_state);
    Ok(())
}

/// Set and persist the preferred system audio device ID.
#[tauri::command]
pub fn set_preferred_system_device(
    device_id: String,
    state: State<'_, Mutex<AppState>>,
) -> Result<(), String> {
    let mut app_state = state.lock().map_err(|e| format!("Lock error: {}", e))?;
    app_state.preferred_system_device_id = Some(device_id);
    save_current_settings(&app_state);
    Ok(())
}

/// Set and persist the output directory for transcript files.
///
/// Pass an empty string to reset to the default (`~/StenoJot/`).
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

/// Set and persist the Whisper model name.
///
/// Valid model names: "tiny", "base", "small", "medium", "large", "large-v3-turbo".
/// Quantized variants like "base-q5_1" and "small-q5_1" are also accepted.
#[tauri::command]
pub fn set_whisper_model(model: String, state: State<'_, Mutex<AppState>>) -> Result<(), String> {
    let mut app_state = state.lock().map_err(|e| format!("Lock error: {}", e))?;
    app_state.whisper_model = model;
    save_current_settings(&app_state);
    Ok(())
}

/// Set and persist the active STT engine.
#[tauri::command]
pub fn set_stt_engine(engine: String, state: State<'_, Mutex<AppState>>) -> Result<(), String> {
    let mut app_state = state.lock().map_err(|e| format!("Lock error: {}", e))?;
    // Validate the engine name
    let _ = crate::transcription::engine::parse_engine(&engine);
    app_state.stt_engine = engine;
    save_current_settings(&app_state);
    Ok(())
}

/// Set and persist the model for the current non-Whisper STT engine.
#[tauri::command]
pub fn set_stt_model(model: String, state: State<'_, Mutex<AppState>>) -> Result<(), String> {
    let mut app_state = state.lock().map_err(|e| format!("Lock error: {}", e))?;
    app_state.stt_model = if model.is_empty() { None } else { Some(model) };
    save_current_settings(&app_state);
    Ok(())
}

/// List available models for a given STT engine.
#[tauri::command]
pub fn get_engine_models(engine: String) -> Vec<manager::ModelEntry> {
    let engine = crate::transcription::engine::parse_engine(&engine);
    manager::get_engine_models(engine)
}

/// Set and persist the initial prompt for Whisper transcription.
///
/// The prompt provides context (domain terms, names, jargon) to improve
/// recognition accuracy. Pass an empty string to clear.
#[tauri::command]
pub fn set_initial_prompt(prompt: String, state: State<'_, Mutex<AppState>>) -> Result<(), String> {
    let mut app_state = state.lock().map_err(|e| format!("Lock error: {}", e))?;
    app_state.initial_prompt = if prompt.is_empty() {
        None
    } else {
        Some(prompt)
    };
    save_current_settings(&app_state);
    Ok(())
}

/// Set and persist the maximum segment duration in seconds.
///
/// Clamped to 1–30 seconds. Larger values reduce Whisper startup overhead
/// but increase latency before transcription appears.
#[tauri::command]
pub fn set_max_segment_seconds(
    seconds: u32,
    state: State<'_, Mutex<AppState>>,
) -> Result<(), String> {
    let mut app_state = state.lock().map_err(|e| format!("Lock error: {}", e))?;
    app_state.max_segment_seconds = seconds.clamp(1, 30);
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

    // Determine which engine + model to use
    let (engine, model_id) = app_state.resolve_engine_and_model();

    // Ensure the model is available before starting
    if !manager::engine_model_exists(engine, &model_id) {
        return Err(format!(
            "{} model '{}' not downloaded. Download it from Settings first.",
            engine, model_id
        ));
    }
    let model_path = manager::get_engine_model_path(engine, &model_id);

    // Start mic capture with current gain setting
    let mic_handle = capture::start_capture(&mic_device_id, Arc::clone(&app_state.mic_gain))
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
    let worker = TranscriptionWorker::start(WorkerConfig {
        engine,
        model_path,
        mic_consumer: mic_handle.consumer,
        system_consumer,
        mic_sample_rate: app_state.mic_sample_rate,
        mic_channels: app_state.mic_channels,
        system_sample_rate: app_state.system_sample_rate,
        system_channels: app_state.system_channels,
        vad_threshold: Arc::clone(&app_state.vad_threshold),
        silence_timeout_seconds: app_state.silence_timeout_seconds,
        initial_prompt: app_state.initial_prompt.clone(),
        max_segment_seconds: app_state.max_segment_seconds,
        channel: on_transcript,
    })?;

    let start_time = chrono::Local::now();
    let loaded = settings::load(&app_state.config_dir);
    let output_dir = loaded.output_dir_resolved();
    let meeting_name = markdown::resolve_meeting_name(&start_time);

    // Create the transcript file immediately with just the header
    let transcript_path =
        markdown::write_transcript(&output_dir, &[], &meeting_name, start_time, start_time)?;

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
#[derive(serde::Serialize, Clone)]
pub struct StopRecordingResult {
    /// Path to the saved transcript file, if any segments were recorded.
    pub transcript_path: Option<String>,
    /// Number of transcript segments in the recording.
    pub segment_count: usize,
}

/// Stop the current recording session.
///
/// Immediately stops audio streams and resets recording state, then spawns
/// a background thread to join the transcription worker, write the final
/// transcript, and optionally generate a summary. Emits a
/// `recording-stopped` event with the [`StopRecordingResult`] when the
/// background work completes.
#[tauri::command]
pub fn stop_recording(
    state: State<'_, Mutex<AppState>>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let mut app_state = state.lock().map_err(|e| format!("Lock error: {}", e))?;

    if !app_state.is_recording {
        return Err(CaptureError::NotRecording.to_string());
    }

    // Stop audio capture streams (fast — just drops handles)
    app_state.mic_stream = None;
    let mut system_capture = app_state.system_capture.take();
    app_state.mic_device_id = None;
    app_state.system_device_id = None;
    app_state.is_recording = false;
    app_state.is_paused = false;

    // Take ownership of worker and recording metadata for background processing
    let mut worker = app_state.worker.take();
    let transcript_path = app_state.current_transcript_path.take();
    let meeting_name = app_state.current_meeting_name.take();
    let start_time = app_state
        .recording_start_time
        .take()
        .unwrap_or_else(chrono::Local::now);

    // Reset RMS levels
    app_state.mic_rms.store(0u32, Ordering::Relaxed);
    app_state.system_rms.store(0u32, Ordering::Relaxed);

    // Gather summary config while we still hold the lock
    let auto_summary = app_state.auto_summary;
    let llm_config = if auto_summary {
        Some(LlmConfig {
            provider: parse_provider(&app_state.llm_provider),
            model: app_state.llm_model.clone().unwrap_or_default(),
            api_key: app_state.llm_api_key.clone(),
            base_url: app_state.llm_base_url.clone(),
        })
    } else {
        None
    };
    let config_dir = app_state.config_dir.clone();

    // Release the lock before spawning the background thread
    drop(app_state);

    let app_handle = app.clone();
    std::thread::spawn(move || {
        // Stop system capture thread (may block briefly)
        if let Some(ref mut sys) = system_capture {
            sys.stop();
        }
        drop(system_capture);

        // Join the transcription worker and collect final segments (blocking)
        let segments = if let Some(ref mut w) = worker {
            w.stop()
        } else {
            Vec::new()
        };
        drop(worker);

        let end_time = chrono::Local::now();
        let name_str = meeting_name.as_deref().unwrap_or("Meeting").to_string();
        let segment_count = segments.len();

        // Write final transcript
        let path_str = if let Some(ref path) = transcript_path {
            if let Err(e) =
                markdown::update_transcript(path, &segments, &name_str, start_time, end_time)
            {
                eprintln!("Failed to write final transcript: {}", e);
            }
            Some(path.to_string_lossy().to_string())
        } else {
            None
        };

        // Notify frontend that recording stop is complete
        let _ = app_handle.emit(
            "recording-stopped",
            StopRecordingResult {
                transcript_path: path_str.clone(),
                segment_count,
            },
        );
        let _ = app_handle.emit("meetings-changed", ());

        // Generate summary if enabled
        if let Some(config) = llm_config {
            if let Some(ref tx_path_str) = path_str {
                let tx_path = PathBuf::from(tx_path_str);
                let loaded = settings::load(&config_dir);
                let out_dir = loaded.output_dir_resolved();
                let name = name_str.clone();

                let _ = app_handle.emit("summary-generating", ());
                match llm::summary::generate_summary(
                    &config, &tx_path, &out_dir, start_time, end_time, &name,
                ) {
                    Ok(result) => {
                        let _ = app_handle.emit(
                            "summary-generated",
                            SummaryEvent {
                                transcript_path: result
                                    .transcript_path
                                    .to_string_lossy()
                                    .to_string(),
                                summary_path: result.summary_path.to_string_lossy().to_string(),
                                meeting_name: result.meeting_name,
                            },
                        );
                        let _ = app_handle.emit("meetings-changed", ());
                    }
                    Err(e) => {
                        eprintln!("Summary generation failed: {}", e);
                        let _ = app_handle.emit("summary-error", e.to_string());
                    }
                }
            }
        }
    });

    Ok(())
}

/// Event payload emitted when summary generation completes.
#[derive(serde::Serialize, Clone)]
struct SummaryEvent {
    /// Path to the transcript file (may have been renamed).
    transcript_path: String,
    /// Path to the generated summary file.
    summary_path: String,
    /// LLM-generated meeting name.
    meeting_name: String,
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
    app_state
        .mic_gain
        .store(clamped.to_bits(), Ordering::Relaxed);
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

/// Get detailed info about the active model (path, size, download status).
///
/// Uses the current engine setting to determine which model to query.
#[tauri::command]
pub fn get_model_info(state: State<'_, Mutex<AppState>>) -> Result<ModelInfo, String> {
    let app_state = state.lock().map_err(|e| format!("Lock error: {}", e))?;
    let (engine, model_id) = app_state.resolve_engine_and_model();
    Ok(manager::get_engine_model_info(engine, &model_id))
}

/// Delete the downloaded model file or directory.
#[tauri::command]
pub fn delete_model(state: State<'_, Mutex<AppState>>) -> Result<(), String> {
    let app_state = state.lock().map_err(|e| format!("Lock error: {}", e))?;
    let (engine, model_id) = app_state.resolve_engine_and_model();
    manager::delete_engine_model(engine, &model_id)
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

/// Download the currently selected model from Hugging Face.
///
/// Spawns the download on a background thread and returns immediately.
/// Emits `download-progress` events during the download, then either
/// `download-complete` (with the file path) or `download-error` (with
/// an error message) when finished.
#[tauri::command]
pub fn download_model(
    app: tauri::AppHandle,
    state: State<'_, Mutex<AppState>>,
) -> Result<(), String> {
    let app_state = state.lock().map_err(|e| format!("Lock error: {}", e))?;
    let (engine, model_id) = app_state.resolve_engine_and_model();
    drop(app_state); // Release lock before spawning

    let app_handle = app.clone();
    std::thread::spawn(move || {
        let result = match engine {
            crate::transcription::engine::SttEngine::Whisper => {
                manager::download_model_file(&model_id, &app_handle)
            }
            _ => manager::download_onnx_model(engine, &model_id, &app_handle),
        };
        match result {
            Ok(path) => {
                let _ = app_handle.emit("download-complete", path.to_string_lossy().to_string());
            }
            Err(e) => {
                eprintln!("Model download failed: {}", e);
                let _ = app_handle.emit("download-error", e);
            }
        }
    });

    Ok(())
}

/// List meetings in the output directory.
#[tauri::command]
pub fn list_meetings(
    state: State<'_, Mutex<AppState>>,
) -> Result<Vec<markdown::MeetingEntry>, String> {
    let app_state = state.lock().map_err(|e| format!("Lock error: {}", e))?;
    let loaded = settings::load(&app_state.config_dir);
    let output_dir = loaded.output_dir_resolved();
    Ok(markdown::list_meetings_in_dir(&output_dir))
}

/// Read the contents of a transcript file.
#[tauri::command]
pub fn read_meeting_transcript(path: String) -> Result<String, String> {
    std::fs::read_to_string(&path).map_err(|e| format!("Failed to read transcript: {}", e))
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

/// Set and persist the LLM provider for summary generation.
///
/// Valid values: "ollama", "anthropic", "openai". Unknown values default to "ollama".
#[tauri::command]
pub fn set_llm_provider(provider: String, state: State<'_, Mutex<AppState>>) -> Result<(), String> {
    let mut app_state = state.lock().map_err(|e| format!("Lock error: {}", e))?;
    // Validate and normalize the provider string
    let normalized = parse_provider(&provider).to_string();
    app_state.llm_provider = normalized;
    save_current_settings(&app_state);
    Ok(())
}

/// Set and persist the LLM model name override.
///
/// Pass an empty string to use the provider's default model.
#[tauri::command]
pub fn set_llm_model(model: String, state: State<'_, Mutex<AppState>>) -> Result<(), String> {
    let mut app_state = state.lock().map_err(|e| format!("Lock error: {}", e))?;
    app_state.llm_model = if model.is_empty() { None } else { Some(model) };
    save_current_settings(&app_state);
    Ok(())
}

/// Set and persist the API key for cloud LLM providers.
///
/// Pass an empty string to clear the key.
#[tauri::command]
pub fn set_llm_api_key(key: String, state: State<'_, Mutex<AppState>>) -> Result<(), String> {
    let mut app_state = state.lock().map_err(|e| format!("Lock error: {}", e))?;
    app_state.llm_api_key = if key.is_empty() { None } else { Some(key) };
    save_current_settings(&app_state);
    Ok(())
}

/// Set and persist a custom base URL for the LLM provider.
///
/// Pass an empty string to use the provider's default URL.
#[tauri::command]
pub fn set_llm_base_url(url: String, state: State<'_, Mutex<AppState>>) -> Result<(), String> {
    let mut app_state = state.lock().map_err(|e| format!("Lock error: {}", e))?;
    app_state.llm_base_url = if url.is_empty() { None } else { Some(url) };
    save_current_settings(&app_state);
    Ok(())
}

/// Enable or disable automatic summary generation after recording stops.
#[tauri::command]
pub fn set_auto_summary(enabled: bool, state: State<'_, Mutex<AppState>>) -> Result<(), String> {
    let mut app_state = state.lock().map_err(|e| format!("Lock error: {}", e))?;
    app_state.auto_summary = enabled;
    save_current_settings(&app_state);
    Ok(())
}

/// Manually trigger summary generation for an existing transcript.
///
/// Runs in a background thread; emits `summary-generated` or `summary-error`
/// events when complete.
#[tauri::command]
pub fn generate_summary(
    transcript_path: String,
    state: State<'_, Mutex<AppState>>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let app_state = state.lock().map_err(|e| format!("Lock error: {}", e))?;

    let config = LlmConfig {
        provider: parse_provider(&app_state.llm_provider),
        model: app_state.llm_model.clone().unwrap_or_default(),
        api_key: app_state.llm_api_key.clone(),
        base_url: app_state.llm_base_url.clone(),
    };

    let loaded = settings::load(&app_state.config_dir);
    let out_dir = loaded.output_dir_resolved();
    drop(app_state); // Release lock before spawning

    let tx_path = PathBuf::from(&transcript_path);

    // Parse meeting name and start time from the transcript filename
    let filename = tx_path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let meeting_name = filename
        .strip_suffix(markdown::TRANSCRIPT_SUFFIX)
        .and_then(|stem| stem.get(17..))
        .map(|n| n.trim().to_string())
        .unwrap_or_else(|| "Meeting".to_string());

    let now = chrono::Local::now();

    let app_handle = app.clone();
    std::thread::spawn(move || {
        let _ = app_handle.emit("summary-generating", ());
        match llm::summary::generate_summary(&config, &tx_path, &out_dir, now, now, &meeting_name) {
            Ok(result) => {
                let _ = app_handle.emit(
                    "summary-generated",
                    SummaryEvent {
                        transcript_path: result.transcript_path.to_string_lossy().to_string(),
                        summary_path: result.summary_path.to_string_lossy().to_string(),
                        meeting_name: result.meeting_name,
                    },
                );
                let _ = app_handle.emit("meetings-changed", ());
            }
            Err(e) => {
                eprintln!("Summary generation failed: {}", e);
                let _ = app_handle.emit("summary-error", e.to_string());
            }
        }
    });

    Ok(())
}

/// Read the contents of a summary file.
#[tauri::command]
pub fn read_meeting_summary(path: String) -> Result<String, String> {
    std::fs::read_to_string(&path).map_err(|e| format!("Failed to read summary: {}", e))
}

/// Refresh the system tray menu to reflect the current recording state.
///
/// Called by the frontend after recording state transitions (start, stop,
/// pause, resume) so the tray menu stays in sync.
#[tauri::command]
pub fn refresh_tray(app: tauri::AppHandle) {
    crate::tray::refresh_tray_menu(&app);
}
