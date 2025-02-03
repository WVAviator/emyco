mod apu;
mod cpu;
mod display;
mod joypad;
mod memory;
mod ppu;
mod serial;
mod timer;

use std::{
    rc::Rc,
    sync::RwLock,
    time::{Duration, Instant},
};

use cpu::CPU;
use crossbeam::channel::Receiver;
use joypad::Joypad;
use memory::{MemoryBus, SharedMemoryController};
use tauri::AppHandle;

use crate::emulator::{Emulator, EmulatorCommand};

pub struct Gameboy {
    memory: SharedMemoryController,
    joypad: Rc<RwLock<Joypad>>,
    cpu: CPU,
    clock: u32,
}

impl Emulator for Gameboy {
    fn new(rom: Vec<u8>, app_handle: AppHandle) -> Self {
        let joypad = Rc::new(RwLock::new(Joypad::new()));

        let memory = MemoryBus::from_rom(rom, app_handle, joypad.clone()).unwrap();

        let cpu = CPU::new(memory.clone());

        Gameboy {
            memory,
            cpu,
            joypad,
            clock: 0,
        }
    }

    fn start(&mut self, receiver: Receiver<EmulatorCommand>) -> Result<(), anyhow::Error> {
        self.cpu.reboot();

        let cycle_time = Duration::from_secs_f32(
            GlobalConstants::CYCLE_RESOLUTION as f32 / GlobalConstants::SYSTEM_CLOCK_RATE as f32,
        );
        let mut next_cycle = Instant::now();

        loop {
            while Instant::now() < next_cycle {}

            self.clock += GlobalConstants::CYCLE_RESOLUTION;

            if self.clock >= GlobalConstants::INPUT_RESPONSIVENESS {
                self.clock -= GlobalConstants::INPUT_RESPONSIVENESS;

                match receiver.try_recv() {
                    Ok(EmulatorCommand::Start) => {}
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

            next_cycle += cycle_time;
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
    pub const INPUT_RESPONSIVENESS: u32 = 8192;
}
