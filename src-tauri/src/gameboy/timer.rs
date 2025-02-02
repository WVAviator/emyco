use log::{error, trace};

use super::memory::{Interrupt, Register};

const TAC_REGISTER_ADDRESS: u16 = 0xFF07;
const TMA_REGISTER_ADDRESS: u16 = 0xFF06;
const TIMA_REGISTER_ADDRESS: u16 = 0xFF05;
const DIV_REGISTER_ADDRESS: u16 = 0xFF04;

pub struct Timer {
    enabled: bool,
    clock: u16,
    cycles: u32,
    frequency: Frequency,
    tma: u8,
    tima: u8,
    tima_reload: TimaReload,
    pending_interrupts: Option<Interrupt>,
}

#[derive(Debug, PartialEq, Clone, Copy)]
enum TimaReload {
    Idle,
    Reloading,
    Reloaded,
}

impl Timer {
    pub fn new() -> Self {
        Timer {
            enabled: false,
            clock: 0,
            cycles: 0,
            frequency: Frequency::Increment256,
            tma: 0,
            tima: 0,
            tima_reload: TimaReload::Idle,
            pending_interrupts: None,
        }
    }

    fn edge_detect(&self) -> bool {
        let freq_bit = match self.frequency {
            Frequency::Increment256 => 1 << 9,
            Frequency::Increment4 => 1 << 3,
            Frequency::Increment16 => 1 << 5,
            Frequency::Increment64 => 1 << 7,
        };

        (freq_bit & self.clock != 0) && self.enabled
    }

    fn timer_increment(&mut self) {
        self.tima = self.tima.wrapping_add(1);
        if self.tima == 0x00 {
            self.tima_reload = TimaReload::Reloading;
            self.pending_interrupts
                .get_or_insert(Interrupt::empty())
                .insert(Interrupt::TIMER);
        }
    }
}

impl Default for Timer {
    fn default() -> Self {
        Self::new()
    }
}

impl Register for Timer {
    fn tick(&mut self, cycles: u32) {
        self.cycles += cycles;

        while self.cycles >= 4 {
            self.cycles -= 4;

            self.tima_reload = match self.tima_reload {
                TimaReload::Idle => TimaReload::Idle,
                TimaReload::Reloading => {
                    self.tima = self.tma;
                    TimaReload::Reloaded
                }
                TimaReload::Reloaded => TimaReload::Idle,
            };

            let current_edge = self.edge_detect();

            self.clock = self.clock.wrapping_add(4);

            let modified_edge = self.edge_detect();
            if let (true, false) = (current_edge, modified_edge) {
                self.timer_increment()
            }
        }
    }

    fn retrieve_interrupts(&mut self) -> Option<Interrupt> {
        self.pending_interrupts.take()
    }

    fn read(&self, address: u16) -> u8 {
        match address {
            DIV_REGISTER_ADDRESS => ((self.clock & 0xFF00) >> 8) as u8,
            TIMA_REGISTER_ADDRESS => self.tima,
            TMA_REGISTER_ADDRESS => self.tma,
            TAC_REGISTER_ADDRESS => {
                let mut value = 0;
                if self.enabled {
                    value |= 0b0000_0100;
                }
                match self.frequency {
                    Frequency::Increment256 => value,
                    Frequency::Increment4 => value | 1,
                    Frequency::Increment16 => value | 2,
                    Frequency::Increment64 => value | 3,
                }
            }
            _ => {
                error!("Attempted to read from timer with non-timer register address.");
                0
            }
        }
    }

    fn write(&mut self, address: u16, value: u8) {
        match address {
            DIV_REGISTER_ADDRESS => {
                let current_edge = self.edge_detect();
                self.clock = 0;
                let modified_edge = self.edge_detect();
                if let (true, false) = (current_edge, modified_edge) {
                    self.timer_increment();
                };
            }
            TIMA_REGISTER_ADDRESS => {
                match self.tima_reload {
                    TimaReload::Idle => {
                        self.tima = value;
                    }
                    TimaReload::Reloading => {
                        // Writing to TIMA before it has finished reloading cancels the reload
                        self.tima_reload = TimaReload::Idle;
                        self.tima = value;
                    }
                    TimaReload::Reloaded => {
                        // Writes are ignored in the same cycle that TIMA was reloaded
                    }
                }
            }
            TMA_REGISTER_ADDRESS => {
                self.tma = value;
                if self.tima_reload == TimaReload::Reloaded {
                    self.tima = self.tma;
                }
            }
            TAC_REGISTER_ADDRESS => {
                trace!("CPU requested new timer settings in TAC register.");

                let current_edge = self.edge_detect();

                self.enabled = value & 0b0000_0100 != 0;
                self.frequency = match value & 0b0000_0011 {
                    0 => Frequency::Increment256,
                    1 => Frequency::Increment4,
                    2 => Frequency::Increment16,
                    _ => Frequency::Increment64,
                };

                let modified_edge = self.edge_detect();
                if let (true, false) = (current_edge, modified_edge) {
                    self.timer_increment();
                };
            }
            _ => {
                error!("Attempted to write to timer at a non-timer register address.")
            }
        }
    }
}

enum Frequency {
    Increment256,
    Increment4,
    Increment16,
    Increment64,
}
