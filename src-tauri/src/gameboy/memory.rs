pub mod cartridge;
mod mbc;

use bitflags::bitflags;
use cartridge::Cartridge;
use log::{info, trace};

use std::{rc::Rc, sync::RwLock};

use super::{apu::APU, joypad::Joypad, ppu::PPU, serial::Serial, timer::Timer, GlobalConstants};

const BOOT_ROM: &[u8; 256] = include_bytes!("./memory/dmg_boot.bin");

pub type SharedMemoryController = Rc<RwLock<dyn MemoryController>>;

pub trait MemoryController {
    fn read_byte(&self, address: u16) -> u8;
    fn write_byte(&mut self, address: u16, value: u8);

    fn tick(&mut self, _cycles: u32) {}
    fn trigger_interrupt(&mut self, interrupt: Interrupt) {
        trace!("Triggered interrupt: {:?}", interrupt);
        let mut if_register = Interrupt::from_bits_truncate(self.read_byte(0xFF0F));
        if_register.insert(interrupt);
        self.write_byte(0xFF0F, if_register.bits());
    }
}

#[allow(unused_variables)]
pub trait Register {
    fn read(&self, address: u16) -> u8;
    fn write(&mut self, address: u16, value: u8);
    fn tick(&mut self, cycles: u32) {}
    fn retrieve_interrupts(&mut self) -> Option<Interrupt> {
        None
    }
}

#[derive(Debug, PartialEq, Clone)]
pub enum DMAState {
    Active(u16, u16, u8),
    Inactive,
}

bitflags! {
    #[repr(transparent)]
    #[derive(Debug, PartialEq, Clone, Copy)]
    pub struct Interrupt: u8 {
        const VBLANK = 0b0000_0001;
        const LCD = 0b0000_0010;
        const TIMER = 0b0000_0100;
        const SERIAL = 0b0000_1000;
        const JOYPAD = 0b0001_0000;
    }
}

pub struct MemoryBus {
    boot_mode: bool,
    cartridge: Cartridge,
    internal_memory: [u8; 16384],
    dma_state: DMAState,
    pending_cycles: i32,
    timer: Timer,
    joypad: Rc<RwLock<Joypad>>,
    serial: Serial,
    apu: APU,
    ppu: PPU,
}

impl MemoryBus {
    pub fn builder() -> MemoryBusBuilder {
        MemoryBusBuilder {
            ppu: None,
            apu: None,
            cartridge: None,
            timer: None,
            joypad: None,
            serial: None,
        }
    }

    fn raw_read(&self, address: u16) -> u8 {
        match address {
            0x0000..=0x00FF if self.boot_mode => BOOT_ROM[address as usize],
            0x0000..=0x7FFF => self.cartridge.read(address),
            0x8000..=0x9FFF => self.ppu.read(address),
            0xA000..=0xBFFF => self.cartridge.read(address),
            0xFE00..=0xFE9F => self.ppu.read(address),
            0xFF01..=0xFF02 => self.serial.read(address),
            0xFF04..=0xFF07 => self.timer.read(address),
            0xFF40..=0xFF4B => self.ppu.read(address),
            0xFF00 => self.joypad.read().unwrap().read(address),
            0xFF10..=0xFF3F if GlobalConstants::AUDIO_ENABLED => self.apu.read(address),
            0xC000..=0xFFFF => self.internal_memory[(address - 0xC000) as usize],
        }
    }

    fn raw_write(&mut self, address: u16, value: u8) {
        match address {
            0xFF50 if self.boot_mode => {
                info!("Boot mode disabled.");
                self.boot_mode = false;
            }
            0x0000..=0x7FFF => {
                self.cartridge.write(address, value);
            }
            0x8000..=0x9FFF => self.ppu.write(address, value),
            0xA000..=0xBFFF => self.cartridge.write(address, value),
            0xFF01..=0xFF02 => self.serial.write(address, value),
            0xFF10..=0xFF3F if GlobalConstants::AUDIO_ENABLED => self.apu.write(address, value),
            0xFF46 => {
                let orig = (value as u16) << 8;
                let dest = 0xFE00;
                trace!(
                    "Initiating DMA transfer from {:#05x} through {:#05x}.",
                    orig,
                    orig + 159
                );
                self.dma_state = DMAState::Active(orig, dest, 160);
                self.internal_memory[(address - 0xC000) as usize] = value;
            }
            0xFF04..=0xFF07 => self.timer.write(address, value),
            0xFE00..=0xFE9F => self.ppu.write(address, value),
            0xFF40..=0xFF4B => self.ppu.write(address, value),
            0xFF00 => self.joypad.write().unwrap().write(address, value),
            0xC000..=0xFFFF => {
                self.internal_memory[(address - 0xC000) as usize] = value;
            }
        }

        if address != 0xFF0F {
            self.check_interrupts();
        }
    }

    fn check_interrupts(&mut self) {
        let mut interrupts = Interrupt::empty();

        if let Some(interrupt) = self.timer.retrieve_interrupts() {
            interrupts |= interrupt;
        }

        if let Some(interrupt) = self.joypad.write().unwrap().retrieve_interrupts() {
            interrupts |= interrupt;
        }

        if let Some(interrupt) = self.serial.retrieve_interrupts() {
            interrupts |= interrupt;
        }

        if let Some(interrupt) = self.ppu.retrieve_interrupts() {
            interrupts |= interrupt;
        }

        if !interrupts.is_empty() {
            trace!("Collected interrupts: {:#?}", interrupts);
            self.trigger_interrupt(interrupts);
        }
    }
}

impl MemoryController for MemoryBus {
    fn tick(&mut self, cycles: u32) {
        self.ppu.tick(cycles);
        self.timer.tick(cycles);
        self.serial.tick(cycles);
        self.joypad.write().unwrap().tick(cycles);
        self.apu.tick(cycles);

        self.check_interrupts();

        if let DMAState::Active(orig, dest, remaining) = self.dma_state {
            self.pending_cycles -= cycles as i32;
            while self.pending_cycles <= 0 {
                self.pending_cycles += 4;
                let byte = self.raw_read(orig);
                self.raw_write(dest, byte);

                let remaining = remaining - 1;
                if remaining == 0 {
                    trace!("Exiting DMA.");
                    self.dma_state = DMAState::Inactive;
                    self.pending_cycles = 0;
                    return;
                }

                self.dma_state =
                    DMAState::Active(orig.wrapping_add(1), dest.wrapping_add(1), remaining);
            }
        }
    }

    fn read_byte(&self, address: u16) -> u8 {
        match (address, &self.dma_state) {
            (_, DMAState::Inactive) => self.raw_read(address),
            (0xFF46, _) => self.raw_read(address),
            (0xFF80..=0xFFFE, DMAState::Active(_, _, _)) => self.raw_read(address),
            (0xFE00..=0xFE9F, DMAState::Active(_, _, _)) => self.raw_read(address),
            (_, DMAState::Active(_, _, _)) => 0x00,
        }
    }

    fn write_byte(&mut self, address: u16, value: u8) {
        match (address, &self.dma_state) {
            (_, DMAState::Inactive) => self.raw_write(address, value),
            (0xFF80..=0xFFFE, DMAState::Active(_, _, _)) => self.raw_write(address, value),
            (0xFE00..=0xFE9F, DMAState::Active(_, _, _)) => self.raw_write(address, value),
            (_, DMAState::Active(_, _, _)) => {}
        }
    }
}

pub struct TestMemoryBus {
    memory: [u8; 0x10000],
}

impl MemoryController for TestMemoryBus {
    fn read_byte(&self, address: u16) -> u8 {
        self.memory[address as usize]
    }

    fn write_byte(&mut self, address: u16, value: u8) {
        self.memory[address as usize] = value;
    }
}

impl Default for TestMemoryBus {
    fn default() -> TestMemoryBus {
        TestMemoryBus {
            memory: [0; 0x10000],
        }
    }
}

impl TestMemoryBus {
    #[allow(dead_code)]
    pub fn new_shared() -> SharedMemoryController {
        Rc::new(RwLock::new(TestMemoryBus::default()))
    }

    /// To be used for testing - loads the provided ROM directly into address 0x0100 for immediate
    /// program counter execution.
    #[allow(dead_code)]
    pub fn with_test_rom(rom_data: Vec<u8>) -> SharedMemoryController {
        let mut memory = TestMemoryBus::default();
        let rom_size = rom_data.len().min(0x8000);
        memory.memory[0x0100..(rom_size + 0x0100)].copy_from_slice(&rom_data[..rom_size]);

        Rc::new(RwLock::new(memory))
    }
}

pub struct MemoryBusBuilder {
    ppu: Option<PPU>,
    apu: Option<APU>,
    cartridge: Option<Cartridge>,
    joypad: Option<Rc<RwLock<Joypad>>>,
    timer: Option<Timer>,
    serial: Option<Serial>,
}

impl MemoryBusBuilder {
    pub fn ppu(mut self, ppu: PPU) -> Self {
        self.ppu = Some(ppu);
        self
    }
    pub fn apu(mut self, apu: APU) -> Self {
        self.apu = Some(apu);
        self
    }
    pub fn cartridge(mut self, cartridge: Cartridge) -> Self {
        self.cartridge = Some(cartridge);
        self
    }
    pub fn joypad(mut self, joypad: Rc<RwLock<Joypad>>) -> Self {
        self.joypad = Some(joypad);
        self
    }
    pub fn timer(mut self, timer: Timer) -> Self {
        self.timer = Some(timer);
        self
    }
    pub fn serial(mut self, serial: Serial) -> Self {
        self.serial = Some(serial);
        self
    }

    pub fn build(self) -> MemoryBus {
        debug_assert!(self.ppu.is_some(), "Missing PPU in Memory Bus builder.");
        debug_assert!(self.apu.is_some(), "Missing APU in Memory Bus builder.");
        debug_assert!(
            self.cartridge.is_some(),
            "Missing Cartridge in Memory Bus builder."
        );
        debug_assert!(
            self.joypad.is_some(),
            "Missing Joypad in Memory Bus builder."
        );
        debug_assert!(self.timer.is_some(), "Missing Timer in Memory Bus builder.");
        debug_assert!(
            self.serial.is_some(),
            "Missing Serial in Memory Bus builder."
        );

        MemoryBus {
            boot_mode: true,
            cartridge: self.cartridge.unwrap(),
            internal_memory: [0; 16384],
            dma_state: DMAState::Inactive,
            pending_cycles: 0,
            timer: self.timer.unwrap(),
            joypad: self.joypad.unwrap(),
            serial: self.serial.unwrap(),
            apu: self.apu.unwrap(),
            ppu: self.ppu.unwrap(),
        }
    }
}
