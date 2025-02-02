mod gameboy;

use std::fs;

use crossbeam::channel::Sender;
use gameboy::Gameboy;
use log::info;
use tauri::{AppHandle, Manager, State};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![start_gameboy])
        .setup(|app| {
            let app_handle = app.handle().clone();
            let emulator_handle = EmulatorHandle::new(app_handle, "../roms/Pokemon Blue.gb");

            app.manage(emulator_handle);

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[tauri::command]
fn start_gameboy(state: State<EmulatorHandle>) {
    info!("Starting Gameboy...");
    state.start();
}

pub enum EmulatorCommand {
    Start,
    Stop,
}

pub struct EmulatorHandle {
    sender: Sender<EmulatorCommand>,
}

impl EmulatorHandle {
    pub fn new(app_handle: AppHandle, rom_path: &str) -> Self {
        let (tx, rx) = crossbeam::channel::bounded(0);

        let rom = fs::read(rom_path).unwrap();

        std::thread::spawn(move || {
            let mut gameboy = Gameboy::new(rom, app_handle);

            while let Ok(command) = rx.recv() {
                match command {
                    EmulatorCommand::Start => gameboy.start().unwrap(),
                    EmulatorCommand::Stop => break,
                }
            }
        });

        Self { sender: tx }
    }

    pub fn start(&self) {
        self.sender.send(EmulatorCommand::Start).unwrap();
    }
}
