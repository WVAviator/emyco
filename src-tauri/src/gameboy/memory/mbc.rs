use std::fmt::Debug;

#[allow(clippy::upper_case_acronyms)]
pub trait MBC: Debug + Send + Sync {
    fn translate_address(&self, address: u16) -> Option<(u32, BankType)>;
    fn handle_control_write(&mut self, address: u16, value: u8);
}

#[allow(clippy::upper_case_acronyms)]
#[derive(Debug, PartialEq, Clone)]
pub enum BankType {
    RAM,
    ROM,
    RTC(u8),
}

use log::trace;

#[derive(Debug)]
pub struct MBC1 {
    bank_1_register: u8,
    bank_2_register: u8,
    ram_enabled: bool,
    banking_mode: BankingMode,
}

#[derive(Debug, PartialEq, Clone, Copy)]
enum BankingMode {
    Simple,
    Advanced,
}

impl MBC1 {
    pub fn new() -> Self {
        MBC1 {
            // The 5-bit BANK1 register is used as the lower 5 bits of the ROM
            // bank number when the CPU accesses the 0x4000-0x7FFF memory area.
            bank_1_register: 1,
            bank_2_register: 0,
            ram_enabled: false,
            banking_mode: BankingMode::Simple,
        }
    }
}

impl Default for MBC1 {
    fn default() -> Self {
        Self::new()
    }
}

impl MBC for MBC1 {
    fn translate_address(&self, address: u16) -> Option<(u32, BankType)> {
        match address {
            0x0000..=0x3FFF => {
                let rom_bank = match self.banking_mode {
                    BankingMode::Simple => 0,
                    BankingMode::Advanced => (self.bank_2_register << 5) as u32,
                };

                let bank_address = (rom_bank * 0x4000) + (address as u32);
                Some((bank_address, BankType::ROM))
            }
            0x4000..=0x7FFF => {
                let rom_bank = (self.bank_1_register | (self.bank_2_register << 5)) as u32;
                let bank_address = (rom_bank * 0x4000) + ((address as u32) - 0x4000);
                Some((bank_address, BankType::ROM))
            }
            0xA000..=0xBFFF if self.ram_enabled => {
                let ram_bank = match self.banking_mode {
                    BankingMode::Simple => 0,
                    BankingMode::Advanced => self.bank_2_register as u32,
                };

                let bank_address = (ram_bank * 0x2000) + ((address as u32) - 0xA000);
                Some((bank_address, BankType::RAM))
            }
            _ => None,
        }
    }

    fn handle_control_write(&mut self, address: u16, value: u8) {
        match address {
            0x0000..=0x1FFF => {
                self.ram_enabled = (value & 0x0F) == 0x0A;
                trace!(
                    "Cartridge ram enabled with byte {:#03x}: {}",
                    value,
                    self.ram_enabled
                );
            }
            0x2000..=0x3FFF => {
                // Set ROM bank
                self.bank_1_register = (value & 0x1F).max(0x01);

                trace!(
                    "Switch ROM bank to {} by writing a {:#03x}",
                    self.bank_1_register,
                    value
                );
            }
            0x4000..=0x5FFF => {
                self.bank_2_register = value & 0x03;
            }
            0x6000..=0x7FFF => {
                match (value & 0x01) == 0x01 {
                    true => self.banking_mode = BankingMode::Advanced,
                    false => {
                        self.banking_mode = BankingMode::Simple;
                    }
                }

                trace!("Switched banking mode to {:?}", self.banking_mode);
            }
            _ => {}
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct MBC2 {
    ram_enabled: bool,
    rom_bank: u8,
}

impl MBC2 {
    pub fn new() -> Self {
        MBC2 {
            ram_enabled: false,
            rom_bank: 1,
        }
    }
}

impl Default for MBC2 {
    fn default() -> Self {
        Self::new()
    }
}

impl MBC for MBC2 {
    fn translate_address(&self, address: u16) -> Option<(u32, BankType)> {
        match address {
            0x0000..=0x3FFF => Some((address as u32, BankType::ROM)),
            0x4000..=0x7FFF => {
                let bank_address = ((self.rom_bank as u32) * 0x4000) + ((address as u32) - 0x4000);
                Some((bank_address, BankType::ROM))
            }
            0xA000..=0xBFFF if self.ram_enabled => {
                // In MBC2, only the bottom 9 bits of the RAM address are used.
                // As a result, 0xA200..=0xBFFF are echoes of 0xA000..=0xA1FF
                let ram_address = address & 0x01FF;
                Some((ram_address as u32, BankType::RAM))
            }
            _ => None,
        }
    }

    fn handle_control_write(&mut self, address: u16, value: u8) {
        if let 0x0000..=0x3FFF = address {
            // Bit 8 controls whether to enable RAM (bit 8 unset) or change ROM banks (bit 8
            // set)
            match address & 0x0100 {
                0x0000 => match value {
                    0x0A => self.ram_enabled = true,
                    _ => self.ram_enabled = false,
                },
                0x0100 => match value & 0x0F {
                    0 => self.rom_bank = 1,
                    bank => self.rom_bank = bank,
                },
                _ => {}
            }
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct MBC3 {
    rom_bank: u8,
    ram_rtc_bank: u8,
    ram_enabled: bool,
    latch_pending: bool,
    rtc: RTC,
}

impl MBC3 {
    pub fn new() -> Self {
        MBC3 {
            rom_bank: 1,
            ram_rtc_bank: 0,
            ram_enabled: false,
            latch_pending: false,
            rtc: RTC::new(),
        }
    }
}

impl Default for MBC3 {
    fn default() -> Self {
        Self::new()
    }
}

impl MBC for MBC3 {
    fn translate_address(&self, address: u16) -> Option<(u32, BankType)> {
        match address {
            0x0000..=0x3FFF => Some((address as u32, BankType::ROM)),
            0x4000..=0x7FFF => {
                let bank_address = ((self.rom_bank as u32) * 0x4000) + ((address as u32) - 0x4000);
                Some((bank_address, BankType::ROM))
            }
            0xA000..=0xBFFF if self.ram_enabled && (0x00..=0x03).contains(&self.ram_rtc_bank) => {
                let bank_address =
                    ((self.ram_rtc_bank as u32) * 0x2000) + ((address as u32) - 0xA000);
                Some((bank_address, BankType::RAM))
            }
            0xA000..=0xBFFF if self.ram_enabled && (0x08..=0x0C).contains(&self.ram_rtc_bank) => {
                let value = match self.ram_rtc_bank {
                    0x08 => self.rtc.read_secs(),
                    0x09 => self.rtc.read_minutes(),
                    0x0A => self.rtc.read_hours(),
                    0x0B => self.rtc.read_day_low(),
                    0x0C => self.rtc.read_day_high(),
                    _ => 0,
                };

                Some((address as u32, BankType::RTC(value)))
            }
            _ => None,
        }
    }

    fn handle_control_write(&mut self, address: u16, value: u8) {
        match address {
            0x0000..=0x1FFF => match value {
                0x0A => self.ram_enabled = true,
                _ => self.ram_enabled = false,
            },
            0x2000..=0x3FFF => match value & 0x7F {
                0x00 => self.rom_bank = 1,
                bank => self.rom_bank = bank,
            },
            0x4000..=0x5FFF => match value {
                0x00..=0x03 => self.ram_rtc_bank = value,
                0x08..=0x0C => self.ram_rtc_bank = value,
                _ => {}
            },
            0x6000..=0x7FFF => match value {
                0x00 => self.latch_pending = true,
                0x01 if self.latch_pending => {
                    self.rtc.latch();
                    self.latch_pending = false;
                }
                _ => self.latch_pending = false,
            },
            0xA000..=0xBFFF => match self.ram_rtc_bank {
                0x08 => self.rtc.write_secs(value),
                0x09 => self.rtc.write_minutes(value),
                0x0A => self.rtc.write_hours(value),
                0x0B => self.rtc.write_day_low(value),
                0x0C => self.rtc.write_day_high(value),
                _ => {}
            },
            _ => {}
        }
    }
}

#[derive(Debug)]
pub struct NoMBC {}

impl NoMBC {
    pub fn new() -> Self {
        NoMBC {}
    }
}

impl Default for NoMBC {
    fn default() -> Self {
        Self::new()
    }
}

impl MBC for NoMBC {
    fn translate_address(&self, address: u16) -> Option<(u32, BankType)> {
        match address {
            0x0000..=0x7FFF => Some((address as u32, BankType::ROM)),
            0xA000..=0xBFFF => Some(((address as u32) - 0xA000, BankType::RAM)),
            _ => None,
        }
    }

    fn handle_control_write(&mut self, _address: u16, _value: u8) {}
}

use chrono::{Days, Duration, NaiveDateTime, Timelike, Utc};

#[allow(clippy::upper_case_acronyms)]
#[derive(Debug, PartialEq, Clone)]
pub struct RTC {
    real_time_offset_secs: i64,
    activation_date: NaiveDateTime,
    latched_time: Option<NaiveDateTime>,
    halted_time: NaiveDateTime,
    pub halted: bool,
    pub day_carry: bool,
}

impl RTC {
    pub fn new() -> Self {
        RTC {
            real_time_offset_secs: 0,
            activation_date: Utc::now().naive_utc(),
            latched_time: None,
            halted_time: Utc::now().naive_utc(),
            halted: false,
            day_carry: false,
        }
    }

    /// A latch saves the current time for reading.
    pub fn latch(&mut self) {
        let now = Utc::now().naive_utc();
        let offset = Duration::seconds(self.real_time_offset_secs);
        if let Some(latched) = now.checked_add_signed(offset) {
            self.latched_time = Some(latched);
            if self.read_days() >= 512 {
                self.day_carry = true;
            }
        }
    }

    /// Returns the number of seconds past the minute of the current time.
    pub fn read_secs(&self) -> u8 {
        match self.latched_time {
            Some(time) => time.second() as u8,
            None => 0,
        }
    }

    /// Returns the number of minutes past the hour of the current time.
    pub fn read_minutes(&self) -> u8 {
        match self.latched_time {
            Some(time) => time.minute() as u8,
            None => 0,
        }
    }

    // Returns the hour of the day in 24-hr format.
    pub fn read_hours(&self) -> u8 {
        match self.latched_time {
            Some(time) => time.hour() as u8,
            None => 0,
        }
    }

    /// Returns the number of days passed since the RTC was initialized.
    pub fn read_days(&self) -> u16 {
        match self.latched_time {
            Some(time) => {
                let diff_days = time.signed_duration_since(self.activation_date).num_days();
                if diff_days < 0 {
                    0
                } else {
                    diff_days as u16
                }
            }
            None => 0,
        }
    }

    // Returns the "day low" byte of the current day
    pub fn read_day_low(&self) -> u8 {
        let days = std::cmp::min(self.read_days(), 511);
        (days & 0xFF) as u8
    }

    /// Returns the "day high" byte of the RTC which inclues flags for halt and day carry
    pub fn read_day_high(&self) -> u8 {
        let days = self.read_days();
        let mut day_high = ((days & 0x0100) >> 8) as u8;
        if self.halted {
            day_high |= 0b0100_0000;
        }
        if self.day_carry {
            day_high |= 0b1000_0000;
        }

        day_high
    }

    /// Halts the RTC. It will not tick while halted. It does this by keeping track of the duration
    /// of the halt and adding a real time offset to account for it.
    pub fn halt(&mut self) {
        self.halted = true;
        self.halted_time = Utc::now().naive_utc();
    }

    /// Unhalts the RTC. Based on the duration since the halt began, a number of offset seconds are
    /// added so that time resumes at the same time it was halted.
    pub fn unhalt(&mut self) {
        self.halted = false;
        let unhalt_time = Utc::now().naive_utc();
        self.real_time_offset_secs -= unhalt_time
            .signed_duration_since(self.halted_time)
            .num_seconds();
    }

    /// Writes seconds to the RTC. This creates a +/- offset based on the current real time seconds.
    pub fn write_secs(&mut self, secs: u8) {
        let current_secs = Utc::now().naive_utc().second() as i64;
        self.real_time_offset_secs += (secs as i64) - current_secs;
    }

    /// Writes minutes to the RTC. This creates a +/- offset based on the current real time minutes.
    pub fn write_minutes(&mut self, minutes: u8) {
        let current_minutes = Utc::now().naive_utc().minute() as i64;
        self.real_time_offset_secs += ((minutes as i64) - current_minutes) * 60;
    }

    /// Writes hours to the RTC. This creates a +/- offset based on the current real time hours.
    pub fn write_hours(&mut self, hours: u8) {
        let current_hours = Utc::now().naive_utc().hour() as i64;
        self.real_time_offset_secs += ((hours as i64) - current_hours) * 3600;
    }

    /// Writes days to the RTC. This updates the "activation date" of the RTC to reflect the new
    /// days value.
    pub fn write_days(&mut self, days: u16) {
        self.activation_date = Utc::now()
            .naive_utc()
            .checked_sub_days(Days::new(days as u64))
            .unwrap_or(self.activation_date);
    }

    /// Writes to the day low value. If the day high bit is set, it will remain unchanged.
    pub fn write_day_low(&mut self, days: u8) {
        let current_days = {
            let diff_days = Utc::now()
                .naive_utc()
                .signed_duration_since(self.activation_date)
                .num_days();
            if diff_days < 0 {
                0
            } else {
                diff_days as u16
            }
        };

        let new_days = days as u16 | (0x0100 & current_days);

        self.activation_date = Utc::now()
            .naive_utc()
            .checked_sub_days(Days::new(new_days as u64))
            .unwrap_or(self.activation_date);
    }

    /// Writes to the day high value. Will trigger a halt if the halt bit is set.
    pub fn write_day_high(&mut self, dh: u8) {
        let current_days = {
            let diff_days = Utc::now()
                .naive_utc()
                .signed_duration_since(self.activation_date)
                .num_days();
            if diff_days < 0 {
                0
            } else {
                diff_days as u16
            }
        };

        if current_days & 0x0100 != 0 && dh & 0x01 == 0 {
            let new_days = current_days & 0x00FF;

            self.activation_date = Utc::now()
                .naive_utc()
                .checked_sub_days(Days::new(new_days as u64))
                .unwrap_or(self.activation_date);
        } else if current_days & 0x0100 == 0 && dh & 0x01 != 0 {
            let new_days = current_days | 0x0100;

            self.activation_date = Utc::now()
                .naive_utc()
                .checked_sub_days(Days::new(new_days as u64))
                .unwrap_or(self.activation_date);
        }

        match dh & 0b0100_0000 != 0 {
            true => self.halt(),
            false if self.halted => self.unhalt(),
            _ => {}
        }

        if dh & 0b1000_0000 == 0 {
            self.day_carry = false;
        }
    }
}

impl Default for RTC {
    fn default() -> Self {
        Self::new()
    }
}
