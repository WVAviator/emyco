use bitflags::bitflags;
use log::{debug, trace};

use super::{
    memory::{Interrupt, Register},
    GlobalConstants,
};

pub const JOYP_REGISTER_ADDRESS: u16 = 0xFF00;

pub struct Joypad {
    dpad: JOYP,
    buttons: JOYP,
    dpad_selected: bool,
    buttons_selected: bool,
    clock: u32,
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
            clock: 0,
            pending_interrupts: None,
        }
    }

    pub fn keydown(&mut self, action: JoypadAction) {
        debug!("Registered key down: {:?}", action);
        match action {
            JoypadAction::Up => self.dpad.remove(JOYP::UP_SELECT),
            JoypadAction::Down => self.dpad.remove(JOYP::DOWN_START),
            JoypadAction::Left => self.dpad.remove(JOYP::LEFT_B),
            JoypadAction::Right => self.dpad.remove(JOYP::RIGHT_A),
            JoypadAction::Start => self.buttons.remove(JOYP::DOWN_START),
            JoypadAction::Select => self.buttons.remove(JOYP::UP_SELECT),
            JoypadAction::B => self.buttons.remove(JOYP::LEFT_B),
            JoypadAction::A => self.buttons.remove(JOYP::RIGHT_A),
        }
    }

    pub fn keyup(&mut self, action: JoypadAction) {
        debug!("Registered key up: {:?}", action);
        match action {
            JoypadAction::Up => self.dpad.insert(JOYP::UP_SELECT),
            JoypadAction::Down => self.dpad.insert(JOYP::DOWN_START),
            JoypadAction::Left => self.dpad.insert(JOYP::LEFT_B),
            JoypadAction::Right => self.dpad.insert(JOYP::RIGHT_A),
            JoypadAction::Start => self.buttons.insert(JOYP::DOWN_START),
            JoypadAction::Select => self.buttons.insert(JOYP::UP_SELECT),
            JoypadAction::B => self.buttons.insert(JOYP::LEFT_B),
            JoypadAction::A => self.buttons.insert(JOYP::RIGHT_A),
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

    fn tick(&mut self, cycles: u32) {
        self.clock += cycles;
        if self.clock < GlobalConstants::JOYPAD_INPUT_RESPONSIVENESS {
            return;
        }
        self.clock -= GlobalConstants::JOYPAD_INPUT_RESPONSIVENESS;

        // let events = self.event_pump.poll_iter().collect::<Vec<Event>>();
        //
        // for event in events {
        //     match event {
        //         Event::Quit { .. } => {
        //             std::process::exit(0);
        //         }
        //         Event::KeyDown {
        //             keycode: Some(keycode),
        //             ..
        //         } => {
        //             match keycode {
        //                 Keycode::UP | Keycode::W => self.keydown(JoypadAction::Up),
        //                 Keycode::DOWN | Keycode::S => self.keydown(JoypadAction::Down),
        //                 Keycode::LEFT | Keycode::A => self.keydown(JoypadAction::Left),
        //                 Keycode::RIGHT | Keycode::D => self.keydown(JoypadAction::Right),
        //                 Keycode::ESCAPE => self.keydown(JoypadAction::Start),
        //                 Keycode::TAB => self.keydown(JoypadAction::Select),
        //                 Keycode::RETURN => self.keydown(JoypadAction::A),
        //                 Keycode::LSHIFT | Keycode::RShift => self.keydown(JoypadAction::B),
        //                 _ => {}
        //             }
        //             self.pending_interrupts
        //                 .get_or_insert(Interrupt::empty())
        //                 .insert(Interrupt::JOYPAD);
        //         }
        //         Event::KeyUp {
        //             keycode: Some(keycode),
        //             ..
        //         } => match keycode {
        //             Keycode::UP | Keycode::W => self.keyup(JoypadAction::Up),
        //             Keycode::DOWN | Keycode::S => self.keyup(JoypadAction::Down),
        //             Keycode::LEFT | Keycode::A => self.keyup(JoypadAction::Left),
        //             Keycode::RIGHT | Keycode::D => self.keyup(JoypadAction::Right),
        //             Keycode::ESCAPE => self.keyup(JoypadAction::Start),
        //             Keycode::TAB => self.keyup(JoypadAction::Select),
        //             Keycode::RETURN => self.keyup(JoypadAction::A),
        //             Keycode::LSHIFT | Keycode::RShift => self.keyup(JoypadAction::B),
        //             _ => {}
        //         },
        //         _ => {}
        //     }
        // }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub enum JoypadAction {
    Up,
    Down,
    Left,
    Right,
    Start,
    Select,
    B,
    A,
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
