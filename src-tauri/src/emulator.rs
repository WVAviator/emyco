use std::{fs, path::PathBuf, sync::Mutex};

use anyhow::bail;
use crossbeam::channel::{Receiver, Sender};
use log::{error, info, warn};
use tauri::{path::PathResolver, AppHandle, Manager, State};

use crate::gameboy::Gameboy;

pub trait Emulator {
    fn new(rom: Vec<u8>, app_handle: AppHandle) -> Self;
    fn start(&mut self, receiver: Receiver<EmulatorCommand>) -> Result<(), anyhow::Error>;
}

pub struct AppState {
    emulator_handle: Option<EmulatorHandle>,
}

impl AppState {
    pub fn new() -> Self {
        AppState {
            emulator_handle: None,
        }
    }
}

#[tauri::command]
pub fn setup_gameboy(state: State<Mutex<AppState>>, app_handle: AppHandle, name: String) {
    if let Ok(app_dir) = app_handle.path().app_data_dir() {
        let rom_path = app_dir.join(name).join("rom.gb");

        if rom_path.exists() {
            let mut state = state.lock().unwrap();
            state.emulator_handle = Some(EmulatorHandle::new::<Gameboy>(app_handle, rom_path));
        }
    }
}

#[tauri::command]
pub fn start_emulator(state: State<Mutex<AppState>>) {
    info!("Starting emulator");
    let state = state.lock().unwrap();
    if let Some(ref emulator_handle) = state.emulator_handle {
        emulator_handle.start();
    } else {
        warn!("No emulator loaded!")
    }
}

#[tauri::command]
pub fn stop_emulator(state: State<Mutex<AppState>>) {
    info!("Stopping emulator");
    let state = state.lock().unwrap();
    if let Some(ref emulator_handle) = state.emulator_handle {
        emulator_handle.stop();
    } else {
        warn!("No emulator loaded!")
    }
}

pub enum EmulatorCommand {
    Start,
    Stop,
}

pub struct EmulatorHandle {
    sender: Sender<EmulatorCommand>,
}

impl EmulatorHandle {
    pub fn new<E: Emulator>(app_handle: AppHandle, rom_path: PathBuf) -> Self {
        let (tx, rx) = crossbeam::channel::bounded(0);

        let rom = fs::read(rom_path).unwrap();

        std::thread::spawn(move || {
            let mut emulator = E::new(rom, app_handle);

            if let Ok(command) = rx.recv() {
                match command {
                    EmulatorCommand::Start => emulator.start(rx).unwrap(),
                    EmulatorCommand::Stop => {}
                }
            }
        });

        Self { sender: tx }
    }

    pub fn start(&self) {
        self.sender.send(EmulatorCommand::Start).unwrap();
    }

    pub fn stop(&self) {
        self.sender.send(EmulatorCommand::Stop).unwrap();
    }
}
