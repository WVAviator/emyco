mod emulator;
mod gameboy;

use std::sync::Mutex;

use emulator::{setup_gameboy, start_emulator, stop_emulator, AppState};
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            setup_gameboy,
            start_emulator,
            stop_emulator
        ])
        .setup(|app| {
            let app_state = Mutex::new(AppState::new());

            app.manage(app_state);

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
