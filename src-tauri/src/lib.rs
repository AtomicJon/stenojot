mod audio;
mod commands;
mod markdown;
mod settings;
mod transcription;

use commands::AppState;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Mutex;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(Mutex::new(AppState::new()))
        .setup(|app| {
            use tauri::Manager;
            let config_dir = app
                .path()
                .app_config_dir()
                .unwrap_or_else(|_| PathBuf::from("/tmp/echonotes"));
            std::fs::create_dir_all(&config_dir)
                .expect("Failed to create app config directory");

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

            if let Some(ref dir) = persisted.models_dir {
                let _ = transcription::manager::set_models_dir(PathBuf::from(dir));
            }

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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
