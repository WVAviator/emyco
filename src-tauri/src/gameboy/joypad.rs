use bitflags::bitflags;
use log::{debug, trace};

use crate::emulator::EmulatorInput;

use super::memory::{Interrupt, Register};

pub struct Joypad {
    dpad: JOYP,
    buttons: JOYP,
    dpad_selected: bool,
    buttons_selected: bool,
    pending_interrupts: Option<Interrupt>,
}

impl Joypad {
    pub fn new() -> Self {
        let mut dpad = JOYP::all();
        dpad.remove(JOYP::SELECT_DPAD);
        let mut buttons = JOYP::all();
        buttons.remove(JOYP::SELECT_BUTTONS);

        Joypad {
            dpad,
            buttons,
            dpad_selected: false,
            buttons_selected: false,
            pending_interrupts: None,
        }
    }

    pub fn keydown(&mut self, input: EmulatorInput) {
        debug!("Registered key down: {:?}", input);
        match input {
            EmulatorInput::Up => self.dpad.remove(JOYP::UP_SELECT),
            EmulatorInput::Down => self.dpad.remove(JOYP::DOWN_START),
            EmulatorInput::Left => self.dpad.remove(JOYP::LEFT_B),
            EmulatorInput::Right => self.dpad.remove(JOYP::RIGHT_A),
            EmulatorInput::Start => self.buttons.remove(JOYP::DOWN_START),
            EmulatorInput::Select => self.buttons.remove(JOYP::UP_SELECT),
            EmulatorInput::B => self.buttons.remove(JOYP::LEFT_B),
            EmulatorInput::A => self.buttons.remove(JOYP::RIGHT_A),
        }
    }

    pub fn keyup(&mut self, input: EmulatorInput) {
        debug!("Registered key up: {:?}", input);
        match input {
            EmulatorInput::Up => self.dpad.insert(JOYP::UP_SELECT),
            EmulatorInput::Down => self.dpad.insert(JOYP::DOWN_START),
            EmulatorInput::Left => self.dpad.insert(JOYP::LEFT_B),
            EmulatorInput::Right => self.dpad.insert(JOYP::RIGHT_A),
            EmulatorInput::Start => self.buttons.insert(JOYP::DOWN_START),
            EmulatorInput::Select => self.buttons.insert(JOYP::UP_SELECT),
            EmulatorInput::B => self.buttons.insert(JOYP::LEFT_B),
            EmulatorInput::A => self.buttons.insert(JOYP::RIGHT_A),
        }
    }
}

impl Default for Joypad {
    fn default() -> Self {
        Self::new()
    }
}

impl Register for Joypad {
    fn read(&self, _address: u16) -> u8 {
        let value = match (self.dpad_selected, self.buttons_selected) {
            (true, true) => self.dpad.intersection(self.buttons).bits(),
            (true, false) => self.dpad.bits(),
            (false, true) => self.buttons.bits(),
            (false, false) => JOYP::all().bits(),
        };
        trace!("Joypad read: {:#09b}", value);

        value
    }

    fn write(&mut self, _address: u16, value: u8) {
        self.dpad_selected = value & JOYP::SELECT_DPAD.bits() == 0;
        self.buttons_selected = value & JOYP::SELECT_BUTTONS.bits() == 0;
    }

    fn retrieve_interrupts(&mut self) -> Option<Interrupt> {
        self.pending_interrupts.take()
    }
}

bitflags! {
    #[repr(transparent)]
    #[derive(Debug, PartialEq, Clone, Copy)]
    struct JOYP: u8 {
        const SELECT_BUTTONS = 0b0010_0000;
        const SELECT_DPAD = 0b0001_0000;
        const DOWN_START = 0b0000_1000;
        const UP_SELECT = 0b0000_0100;
        const LEFT_B = 0b0000_0010;
        const RIGHT_A = 0b0000_0001;
    }
}
