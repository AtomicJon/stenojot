use cpal::Stream;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use tauri::State;

use crate::audio::capture;
use crate::audio::pipeline::AudioPipeline;
use crate::audio::types::{AudioDevice, AudioLevels, CaptureError};

/// Application state managed by Tauri.
pub struct AppState {
    pub is_recording: bool,
    pub mic_device_id: Option<String>,
    pub system_device_id: Option<String>,
    pub mic_rms: Arc<AtomicU32>,
    pub system_rms: Arc<AtomicU32>,
    pub mic_stream: Option<Stream>,
    pub system_stream: Option<Stream>,
    pub pipeline: AudioPipeline,
}

// Safety: cpal::Stream is !Send and !Sync, but we only ever access it
// while holding the Mutex lock from a single thread at a time.
// The streams themselves are audio device handles that are safe to drop
// from any thread.
unsafe impl Send for AppState {}
unsafe impl Sync for AppState {}

impl AppState {
    pub fn new() -> Self {
        Self {
            is_recording: false,
            mic_device_id: None,
            system_device_id: None,
            mic_rms: Arc::new(AtomicU32::new(0)),
            system_rms: Arc::new(AtomicU32::new(0)),
            mic_stream: None,
            system_stream: None,
            pipeline: AudioPipeline::new(),
        }
    }
}

#[tauri::command]
pub fn get_audio_devices() -> Result<Vec<AudioDevice>, String> {
    capture::list_input_devices().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn start_recording(
    mic_device_id: String,
    system_device_id: String,
    state: State<'_, Mutex<AppState>>,
) -> Result<(), String> {
    let mut app_state = state.lock().map_err(|e| format!("Lock error: {}", e))?;

    if app_state.is_recording {
        return Err(CaptureError::AlreadyRecording.to_string());
    }

    // Start mic capture
    let mic_handle = capture::start_capture(&mic_device_id).map_err(|e| e.to_string())?;
    app_state.mic_rms = mic_handle.rms_level;
    app_state.pipeline.set_mic_consumer(mic_handle.consumer);

    // Start system audio capture
    let system_handle = capture::start_capture(&system_device_id).map_err(|e| e.to_string())?;
    app_state.system_rms = system_handle.rms_level;
    app_state.pipeline.set_system_consumer(system_handle.consumer);

    app_state.mic_stream = Some(mic_handle.stream);
    app_state.system_stream = Some(system_handle.stream);
    app_state.mic_device_id = Some(mic_device_id);
    app_state.system_device_id = Some(system_device_id);
    app_state.is_recording = true;

    Ok(())
}

#[tauri::command]
pub fn stop_recording(state: State<'_, Mutex<AppState>>) -> Result<(), String> {
    let mut app_state = state.lock().map_err(|e| format!("Lock error: {}", e))?;

    if !app_state.is_recording {
        return Err(CaptureError::NotRecording.to_string());
    }

    // Dropping the streams stops capture
    app_state.mic_stream = None;
    app_state.system_stream = None;
    app_state.mic_device_id = None;
    app_state.system_device_id = None;
    app_state.is_recording = false;

    // Drain any remaining samples and clear consumers
    app_state.pipeline.drain_buffers();
    app_state.pipeline.clear_consumers();

    // Reset RMS levels
    app_state.mic_rms.store(0u32, Ordering::Relaxed);
    app_state.system_rms.store(0u32, Ordering::Relaxed);

    Ok(())
}

#[tauri::command]
pub fn get_audio_levels(state: State<'_, Mutex<AppState>>) -> Result<AudioLevels, String> {
    let app_state = state.lock().map_err(|e| format!("Lock error: {}", e))?;

    let mic_rms = f32::from_bits(app_state.mic_rms.load(Ordering::Relaxed));
    let system_rms = f32::from_bits(app_state.system_rms.load(Ordering::Relaxed));

    // Drain buffers to prevent overflow while we're recording
    // (In Phase 2 this will be replaced by actual processing)
    drop(app_state);
    let mut app_state = state.lock().map_err(|e| format!("Lock error: {}", e))?;
    if app_state.is_recording {
        app_state.pipeline.drain_buffers();
    }

    Ok(AudioLevels {
        mic_rms,
        system_rms,
    })
}
