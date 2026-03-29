mod audio;
mod commands;
mod llm;
mod markdown;
mod settings;
mod transcription;
mod tray;

use commands::AppState;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Mutex;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(Mutex::new(AppState::new()))
        .setup(|app| {
            let config_dir = app
                .path()
                .app_config_dir()
                .unwrap_or_else(|_| PathBuf::from("/tmp/stenojot"));
            std::fs::create_dir_all(&config_dir).expect("Failed to create app config directory");

            let persisted = settings::load(&config_dir);

            let state = app.state::<Mutex<AppState>>();
            let mut app_state = state.lock().expect("Failed to lock AppState during setup");

            app_state.config_dir = config_dir;
            app_state
                .mic_gain
                .store(persisted.mic_gain.to_bits(), Ordering::Relaxed);
            app_state
                .vad_threshold
                .store(persisted.vad_threshold.to_bits(), Ordering::Relaxed);
            app_state.preferred_mic_device_id = persisted.mic_device_id;
            app_state.preferred_system_device_id = persisted.system_device_id;

            app_state.output_dir = persisted.output_dir;
            app_state.silence_timeout_seconds = persisted.silence_timeout_seconds;
            app_state.stt_engine = persisted.stt_engine;
            app_state.whisper_model = persisted.whisper_model;
            app_state.stt_model = persisted.stt_model;
            app_state.initial_prompt = persisted.initial_prompt;
            app_state.max_segment_seconds = persisted.max_segment_seconds;
            app_state.llm_provider = persisted.llm_provider;
            app_state.llm_model = persisted.llm_model;
            app_state.llm_api_key = persisted.llm_api_key;
            app_state.llm_base_url = persisted.llm_base_url;
            app_state.auto_summary = persisted.auto_summary;

            if let Some(ref dir) = persisted.models_dir {
                let _ = transcription::manager::set_models_dir(PathBuf::from(dir));
            }

            drop(app_state);
            tray::setup_tray(app)?;

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_audio_devices,
            commands::get_system_audio_devices,
            commands::get_settings,
            commands::set_preferred_mic,
            commands::set_preferred_system_device,
            commands::start_recording,
            commands::stop_recording,
            commands::get_audio_levels,
            commands::set_mic_gain,
            commands::get_mic_gain,
            commands::set_vad_threshold,
            commands::get_vad_threshold,
            commands::get_model_info,
            commands::delete_model,
            commands::set_models_dir,
            commands::download_model,
            commands::set_output_dir,
            commands::get_output_dir,
            commands::set_silence_timeout,
            commands::list_meetings,
            commands::read_meeting_transcript,
            commands::pause_recording,
            commands::resume_recording,
            commands::save_current_transcript,
            commands::set_whisper_model,
            commands::set_stt_engine,
            commands::set_stt_model,
            commands::get_engine_models,
            commands::set_initial_prompt,
            commands::set_max_segment_seconds,
            commands::set_llm_provider,
            commands::set_llm_model,
            commands::set_llm_api_key,
            commands::set_llm_base_url,
            commands::set_auto_summary,
            commands::generate_summary,
            commands::read_meeting_summary,
            commands::refresh_tray,
        ])
        // Intercept window close to hide instead of quit, so the app stays in the system tray.
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app, _event| {});
}
