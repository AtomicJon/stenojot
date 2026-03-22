mod audio;
mod commands;
mod transcription;

use commands::AppState;
use std::sync::Mutex;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(Mutex::new(AppState::new()))
        .invoke_handler(tauri::generate_handler![
            commands::get_audio_devices,
            commands::get_system_audio_devices,
            commands::start_recording,
            commands::stop_recording,
            commands::get_audio_levels,
            commands::get_model_info,
            commands::delete_model,
            commands::set_models_dir,
            commands::download_model,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
