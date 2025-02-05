mod apu;
mod cpu;
mod display;
mod joypad;
mod memory;
mod ppu;
mod serial;
mod timer;

use std::{rc::Rc, sync::RwLock};

use apu::APU;
use cpu::CPU;
use crossbeam::channel::Receiver;
use display::WebviewDisplay;
use joypad::Joypad;
use memory::{cartridge::Cartridge, MemoryBus, MemoryController, SharedMemoryController};
use ppu::PPU;
use serial::Serial;
use tauri::{AppHandle, Manager};
use timer::Timer;

use crate::emulator::{Emulator, EmulatorCommand};

pub struct Gameboy {
    memory: SharedMemoryController,
    joypad: Rc<RwLock<Joypad>>,
    cpu: CPU,
    clock: u32,
}

impl Emulator for Gameboy {
    fn new(rom: Vec<u8>, app_handle: AppHandle) -> Self {
        let save_data_path = app_handle.path().local_data_dir().unwrap();
        let joypad = Rc::new(RwLock::new(Joypad::new()));
        let display = Box::new(WebviewDisplay::new(app_handle));
        let memory = MemoryBus::builder()
            .joypad(joypad.clone())
            .cartridge(Cartridge::new(rom, save_data_path).unwrap())
            .serial(Serial::new())
            .apu(APU::new())
            .ppu(PPU::new(display))
            .timer(Timer::new())
            .build();

        let memory = Rc::new(RwLock::new(memory));

        GameboyBuilder::new()
            .cpu(CPU::new(memory.clone()))
            .memory(memory)
            .joypad(joypad)
            .build()
    }

    fn start(&mut self, receiver: &Receiver<EmulatorCommand>) -> Result<(), anyhow::Error> {
        self.cpu.reboot();

        loop {
            self.clock += GlobalConstants::CYCLE_RESOLUTION;

            if self.clock >= GlobalConstants::INPUT_RESPONSIVENESS {
                self.clock -= GlobalConstants::INPUT_RESPONSIVENESS;

                match receiver.try_recv() {
                    Ok(EmulatorCommand::Start) => {}
                    Ok(EmulatorCommand::Pause) => loop {
                        if let Ok(EmulatorCommand::Start) = receiver.recv() {
                            break;
                        }
                    },
                    Ok(EmulatorCommand::Stop) => break,
                    Ok(EmulatorCommand::KeyDown(input)) => {
                        self.joypad.write().unwrap().keydown(input);
                    }
                    Ok(EmulatorCommand::KeyUp(input)) => {
                        self.joypad.write().unwrap().keyup(input);
                    }
                    Err(_) => {}
                };
            }

            self.cpu.tick(GlobalConstants::CYCLE_RESOLUTION);
            self.memory
                .write()
                .unwrap()
                .tick(GlobalConstants::CYCLE_RESOLUTION);
        }

        Ok(())
    }
}

pub struct GlobalConstants;

impl GlobalConstants {
    /// The number of t-cycles that pass every second. Adjusting this will make the emulator run
    /// faster or slower than real Gameboy hardware.
    pub const SYSTEM_CLOCK_RATE: u32 = 4194304;

    /// The number of cycles that pass per iteration of the internal system clock. Setting this to
    /// one would be the most accurate, but sacrifices performance. Higher numbers result in better
    /// performance, but sacrifice accuracy in cycle timings between systems.
    pub const CYCLE_RESOLUTION: u32 = 4;

    /// Whether audio should be enabled at all. If set to false, all memory reads/writes to the APU
    /// registers will be ignored.
    pub const AUDIO_ENABLED: bool = true;

    /// The number of t-cycles that pass before events will be polled. Lower values means
    /// more responsive controls, sacrificing performance
    pub const INPUT_RESPONSIVENESS: u32 = 70224;
}

pub struct GameboyBuilder {
    cpu: Option<CPU>,
    memory: Option<Rc<RwLock<dyn MemoryController>>>,
    joypad: Option<Rc<RwLock<Joypad>>>,
}

impl GameboyBuilder {
    fn new() -> Self {
        GameboyBuilder {
            cpu: None,
            memory: None,
            joypad: None,
        }
    }
    fn cpu(mut self, cpu: CPU) -> Self {
        self.cpu = Some(cpu);
        self
    }

    fn memory(mut self, memory: Rc<RwLock<dyn MemoryController>>) -> Self {
        self.memory = Some(memory);
        self
    }

    fn joypad(mut self, joypad: Rc<RwLock<Joypad>>) -> Self {
        self.joypad = Some(joypad);
        self
    }

    fn build(self) -> Gameboy {
        debug_assert!(self.cpu.is_some(), "No CPU specified on builder.");
        debug_assert!(self.memory.is_some(), "No Memory specified on builder.");
        debug_assert!(self.joypad.is_some(), "No Joypad specified on builder.");

        Gameboy {
            cpu: self.cpu.unwrap(),
            memory: self.memory.unwrap(),
            joypad: self.joypad.unwrap(),
            clock: 4560,
        }
    }
}
