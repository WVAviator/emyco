use super::{
    memory::{Interrupt, Register},
    GlobalConstants,
};

#[derive(Debug)]
pub struct Serial {
    transfer_state: TransferState,
    clock_speed: ClockSpeed,
    sb_register: u8,
    master: bool,
    remaining_cycles: i32,
    pending_interrupts: Option<Interrupt>,
}

impl Serial {
    pub fn new() -> Self {
        Serial {
            transfer_state: TransferState::Waiting,
            clock_speed: ClockSpeed::Hz8192,
            sb_register: 0,
            master: false,
            remaining_cycles: 0,
            pending_interrupts: None,
        }
    }
}

impl Default for Serial {
    fn default() -> Self {
        Self::new()
    }
}

impl Register for Serial {
    fn tick(&mut self, cycles: u32) {
        self.remaining_cycles -= cycles as i32;
        if self.remaining_cycles > 0 {
            return;
        }

        self.remaining_cycles += match self.clock_speed {
            ClockSpeed::Hz8192 => GlobalConstants::SYSTEM_CLOCK_RATE as i32 / 8192,
            ClockSpeed::Hz16384 => GlobalConstants::SYSTEM_CLOCK_RATE as i32 / 16384,
            ClockSpeed::Hz262144 => GlobalConstants::SYSTEM_CLOCK_RATE as i32 / 262144,
            ClockSpeed::Hz524288 => GlobalConstants::SYSTEM_CLOCK_RATE as i32 / 524288,
        };

        match self.transfer_state {
            TransferState::InProgress { byte, shifts } if shifts >= 8 => {
                print!("{}", char::from(byte));
                self.transfer_state = TransferState::Waiting;
                self.pending_interrupts
                    .get_or_insert(Interrupt::empty())
                    .insert(Interrupt::SERIAL);
            }
            TransferState::InProgress { byte, shifts } => {
                self.sb_register <<= 1;
                self.transfer_state = TransferState::InProgress {
                    byte,
                    shifts: shifts + 1,
                };
            }
            TransferState::Waiting => {}
        }
    }

    fn retrieve_interrupts(&mut self) -> Option<Interrupt> {
        self.pending_interrupts.take()
    }

    fn read(&self, address: u16) -> u8 {
        match address {
            0xFF01 => self.sb_register,
            0xFF02 => {
                let mut sc = 0;

                sc |= match self.transfer_state {
                    TransferState::Waiting => 0,
                    _ => 0b1000_0000,
                };

                sc |= match self.clock_speed {
                    ClockSpeed::Hz8192 | ClockSpeed::Hz16384 => 0,
                    ClockSpeed::Hz262144 | ClockSpeed::Hz524288 => 0b0000_0010,
                };

                sc |= match self.master {
                    true => 1,
                    false => 0,
                };

                sc
            }
            _ => panic!("{:#05x} is not a Serial address.", address),
        }
    }

    fn write(&mut self, address: u16, value: u8) {
        match address {
            0xFF01 => self.sb_register = value,
            0xFF02 => {
                if self.transfer_state == TransferState::Waiting && value & 0b1000_0000 != 0 {
                    self.transfer_state = TransferState::InProgress {
                        byte: self.sb_register,
                        shifts: 0,
                    };
                }

                match value & 0b0000_0010 != 0 {
                    true => self.clock_speed = ClockSpeed::Hz262144,
                    false => self.clock_speed = ClockSpeed::Hz8192,
                }

                self.master = value & 0b0000_0001 != 0;
            }
            _ => panic!("{:#05x} is not a Serial address.", address),
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
enum TransferState {
    Waiting,
    InProgress { byte: u8, shifts: u8 },
}

#[allow(dead_code)]
#[derive(Debug, PartialEq, Clone)]
enum ClockSpeed {
    Hz8192,
    Hz16384, // CGB only
    Hz262144,
    Hz524288, // CGB only
}
