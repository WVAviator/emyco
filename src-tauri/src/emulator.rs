use std::{fs, path::PathBuf, sync::Mutex, thread::JoinHandle};

use crossbeam::channel::{Receiver, Sender};
use log::{info, warn};
use tauri::{AppHandle, Manager, State};

use crate::gameboy::Gameboy;

pub trait Emulator {
    fn new(rom: Vec<u8>, app_handle: AppHandle) -> Self;
    fn start(&mut self, receiver: &Receiver<EmulatorCommand>) -> Result<(), anyhow::Error>;
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
pub fn unload_emulator(state: State<Mutex<AppState>>) {
    info!("Request to unload emulator received.");
    let mut state = state.lock().unwrap();

    if let Some(ref mut emulator_handle) = state.emulator_handle {
        info!("Stopping...");
        emulator_handle.stop();
    } else {
        warn!("No emulator loaded!")
    }

    info!("Unsetting emulator reference.");
    state.emulator_handle = None;

    info!("Emulator unloaded.");
}

#[tauri::command]
pub fn setup_gameboy(state: State<Mutex<AppState>>, app_handle: AppHandle, name: String) {
    info!(
        "Request to load Gameboy emulator with ROM {} received.",
        name
    );
    if let Ok(app_dir) = app_handle.path().app_data_dir() {
        let rom_path = app_dir.join(format!("{}.gb", name));

        if rom_path.exists() {
            info!("Identified ROM file at {:?}", rom_path);

            let mut state = state.lock().unwrap();
            state.emulator_handle = Some(EmulatorHandle::new::<Gameboy>(app_handle, rom_path));
            info!("Initialized emulator with ROM {}", name);
        } else {
            warn!("No ROM file found at expected location {:?}", rom_path);
        }
    }
}

#[tauri::command]
pub fn start_emulator(state: State<Mutex<AppState>>) {
    info!("Starting emulator...");
    let state = state.lock().unwrap();
    if let Some(ref emulator_handle) = state.emulator_handle {
        emulator_handle.start();
    } else {
        warn!("No emulator loaded!")
    }
}

#[tauri::command]
pub fn pause_emulator(state: State<Mutex<AppState>>) {
    info!("Stopping emulator");
    let state = state.lock().unwrap();
    if let Some(ref emulator_handle) = state.emulator_handle {
        emulator_handle.pause();
    } else {
        warn!("No emulator loaded!")
    }
}

#[tauri::command]
pub fn stop_emulator(state: State<Mutex<AppState>>) {
    info!("Stopping emulator");
    let mut state = state.lock().unwrap();
    if let Some(ref mut emulator_handle) = state.emulator_handle {
        emulator_handle.stop();
    } else {
        warn!("No emulator loaded!")
    }
}

#[tauri::command]
pub fn register_input(state: State<Mutex<AppState>>, key: String, down: bool) {
    let input = match key.as_str() {
        "start" => EmulatorInput::Start,
        "select" => EmulatorInput::Select,
        "a" => EmulatorInput::A,
        "b" => EmulatorInput::B,
        "up" => EmulatorInput::Up,
        "down" => EmulatorInput::Down,
        "left" => EmulatorInput::Left,
        "right" => EmulatorInput::Right,
        _ => panic!("Invalid input key string {}", key),
    };

    let state = state.lock().unwrap();
    if let Some(ref emulator_handle) = state.emulator_handle {
        emulator_handle.send_input(input, down);
    } else {
        warn!("No emulator loaded!")
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum EmulatorCommand {
    Start,
    Stop,
    Pause,
    KeyDown(EmulatorInput),
    KeyUp(EmulatorInput),
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum EmulatorInput {
    Start,
    Select,
    A,
    B,
    Up,
    Down,
    Left,
    Right,
}

pub struct EmulatorHandle {
    sender: Sender<EmulatorCommand>,
    thread_handle: Option<JoinHandle<()>>,
}

impl EmulatorHandle {
    pub fn new<E: Emulator>(app_handle: AppHandle, rom_path: PathBuf) -> Self {
        let (tx, rx) = crossbeam::channel::bounded(0);

        let rom = fs::read(&rom_path).unwrap();

        let thread_handle = std::thread::spawn(move || {
            let mut emulator = E::new(rom, app_handle);

            if let Ok(command) = rx.recv() {
                if command == EmulatorCommand::Start {
                    emulator.start(&rx).unwrap()
                }
            }
        });

        Self {
            sender: tx,
            thread_handle: Some(thread_handle),
        }
    }

    pub fn start(&self) {
        self.sender.send(EmulatorCommand::Start).unwrap();
    }

    pub fn pause(&self) {
        self.sender.send(EmulatorCommand::Pause).unwrap();
    }

    pub fn stop(&mut self) {
        self.sender.send(EmulatorCommand::Stop).unwrap();
        if let Some(thread_handle) = self.thread_handle.take() {
            thread_handle.join().unwrap();
        }
        info!("Gracefully terminated emulator thread.");
    }

    pub fn send_input(&self, input: EmulatorInput, down: bool) {
        let command = match down {
            true => EmulatorCommand::KeyDown(input),
            false => EmulatorCommand::KeyUp(input),
        };

        self.sender.send(command).unwrap();
    }
}
