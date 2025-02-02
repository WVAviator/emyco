use blip_buf::BlipBuf;

pub trait AudioChannel: Register {
    fn is_enabled(&self) -> bool;
    fn get_samples(&mut self) -> [i16; SAMPLE_BUFFER_SIZE];
}

#[repr(transparent)]
pub struct AmplitudeChange(pub i16);

pub struct PulseChannel {
    enabled: bool,
    sweep: Sweep,
    wave_duty: WaveDuty,
    length_timer: LengthTimer,
    volume_envelope: VolumeEnvelope,
    period: Period,
    current_amplitude: u8,
    last_amplitude: u8,
    blip: BlipBuf,
    clock: u32,
}

impl PulseChannel {
    pub fn new() -> Self {
        let mut blip = BlipBuf::new(2048);
        blip.set_rates(
            GlobalConstants::SYSTEM_CLOCK_RATE as f64,
            SAMPLE_RATE as f64,
        );
        PulseChannel {
            enabled: false,
            sweep: Sweep::from(0),
            wave_duty: WaveDuty::Duty12_5,
            length_timer: LengthTimer::new(64),
            volume_envelope: VolumeEnvelope::new(),
            period: Period::new(4, 8),
            current_amplitude: 0,
            last_amplitude: 0,
            blip,
            clock: 0,
        }
    }

    #[inline]
    fn is_high(&self) -> bool {
        let phase = self.period.get_phase() & 0b0111;
        match self.wave_duty {
            WaveDuty::Duty12_5 => phase == 7,
            WaveDuty::Duty25 => phase >= 6,
            WaveDuty::Duty50 => phase >= 4,
            WaveDuty::Duty75 => phase >= 2,
        }
    }
}

impl Default for PulseChannel {
    fn default() -> Self {
        Self::new()
    }
}

impl Register for PulseChannel {
    fn read(&self, address: u16) -> u8 {
        match address {
            0x0000 => (&self.sweep).into(),
            0x0001 => {
                let wave_duty = match self.wave_duty {
                    WaveDuty::Duty12_5 => 0,
                    WaveDuty::Duty25 => 1,
                    WaveDuty::Duty50 => 2,
                    WaveDuty::Duty75 => 3,
                };

                wave_duty | self.length_timer.get_initial_length()
            }
            0x0002 => (&self.volume_envelope).into(),
            0x0004 => match self.length_timer.is_enabled() {
                true => 0b0100_0000,
                false => 0,
            },
            _ => 0xFF,
        }
    }

    fn write(&mut self, address: u16, value: u8) {
        match address {
            // NRX0: Channel 1 Special
            // This register controls CH1’s period sweep functionality. Only valid in channel 1.
            0x0000 => {
                self.sweep = Sweep::from(value);
            }

            // NRX1: Channel 1 Length Timer and Duty Cycle
            // This register controls both the channel’s length timer and duty cycle
            // (the ratio of the time spent low vs. high). The selected duty cycle also
            // alters the phase, although the effect is hardly noticeable except in combination
            // with other channels.
            0x0001 => {
                self.wave_duty = match (value & 0b1100_0000) >> 6 {
                    0 => WaveDuty::Duty12_5,
                    1 => WaveDuty::Duty25,
                    2 => WaveDuty::Duty50,
                    _ => WaveDuty::Duty75,
                };

                self.length_timer.set_initial_length(value & 0b0011_1111);
            }

            // NRX2: Channel 1 Volume and Envelope
            // This register controls the digital amplitude of the “high” part of the pulse, and the
            // sweep applied to that setting.
            0x0002 => {
                self.volume_envelope.update(value);

                if self.volume_envelope.is_disabled() {
                    self.enabled = false;
                }
            }

            // NRX3: Channel 1 Period Low (Write Only)
            // This register stores the low 8 bits of the channel’s 11-bit “period value”. The upper
            // 3 bits are stored in the low 3 bits of NR14.
            0x0003 => {
                self.period.set_period_lower(value);
            }

            // NRX4: Channel 1 Period High and Control
            0x0004 => {
                if value & 0b1000_0000 != 0 {
                    self.enabled = true;
                    self.length_timer.reset();
                    self.period.reset();
                    self.volume_envelope.reset();
                }

                self.length_timer.set_enabled(value & 0b0100_0000 != 0);

                self.period.set_period_upper(value);
            }
            _ => {}
        }
    }

    fn tick(&mut self, cycles: u32) {
        self.clock += cycles;

        if self.length_timer.tick(cycles) {
            self.enabled = false;
            self.current_amplitude = 0;
        }

        if let Some(VolumeUpdate(volume)) = self.volume_envelope.tick(cycles) {
            if self.is_high() {
                self.current_amplitude = volume;
            }
        }

        let new_period = self.sweep.tick(cycles, self.period.get_period());
        if new_period >= 2048 {
            self.sweep.disable();
        }
        self.period.set_period(new_period);

        if let Some(_phase_update) = self.period.tick(cycles) {
            if self.is_high() {
                self.current_amplitude = self.volume_envelope.get_volume();
            } else {
                self.current_amplitude = 0;
            }
        }

        if self.current_amplitude != self.last_amplitude {
            let delta =
                ((self.current_amplitude as i32) - (self.last_amplitude as i32)) * (32767 / 15);
            self.blip.add_delta(self.clock, delta);
            self.last_amplitude = self.current_amplitude;
        }
    }
}

impl AudioChannel for PulseChannel {
    fn get_samples(&mut self) -> [i16; SAMPLE_BUFFER_SIZE] {
        self.blip.end_frame(self.clock);
        self.clock = 0;

        let mut buffer = [0; SAMPLE_BUFFER_SIZE];

        self.blip.read_samples(&mut buffer, false);

        buffer
    }

    #[inline]
    fn is_enabled(&self) -> bool {
        self.enabled
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
enum WaveDuty {
    Duty12_5,
    Duty25,
    Duty50,
    Duty75,
}

pub struct WaveChannel {
    enabled: bool,
    dac_enabled: bool,
    length_timer: LengthTimer,
    volume: WaveChannelVolume,
    period: Period,
    wave_ram: [u8; 16],
    current_amplitude: u8,
    last_amplitude: u8,
    clock: u32,
    blip: BlipBuf,
}

impl WaveChannel {
    pub fn new() -> Self {
        let mut blip = BlipBuf::new(2048);
        blip.set_rates(
            GlobalConstants::SYSTEM_CLOCK_RATE as f64,
            SAMPLE_RATE as f64,
        );
        WaveChannel {
            enabled: false,
            dac_enabled: true,
            length_timer: LengthTimer::new(256),
            volume: WaveChannelVolume::Mute,
            period: Period::new(2, 32),
            wave_ram: [0; 16],
            last_amplitude: 0,
            current_amplitude: 0,
            clock: 0,
            blip,
        }
    }
}

impl Default for WaveChannel {
    fn default() -> Self {
        Self::new()
    }
}

impl Register for WaveChannel {
    fn read(&self, address: u16) -> u8 {
        match address {
            0xFF1A => match self.dac_enabled {
                true => 0b1000_0000,
                false => 0,
            },
            0xFF1C => match self.volume {
                WaveChannelVolume::Mute => 0b0000_0000,
                WaveChannelVolume::Volume100 => 0b0010_0000,
                WaveChannelVolume::Volume50 => 0b0100_0000,
                WaveChannelVolume::Volume25 => 0b0110_0000,
            },
            0xFF1D => (self.period.get_period() & 0xFF) as u8,
            0xFF1E => match self.length_timer.is_enabled() {
                true => 0b0100_0000,
                false => 0,
            },
            0xFF30..=0xFF3F => {
                if self.enabled {
                    return 0xFF;
                }

                self.wave_ram[address as usize - 0xFF30]
            }
            _ => 0xFF,
        }
    }

    fn write(&mut self, address: u16, value: u8) {
        match address {
            0xFF1A => {
                self.dac_enabled = value & 0b1000_0000 != 0;
                if !self.dac_enabled {
                    self.enabled = false;
                }
            }
            0xFF1B => {
                self.length_timer.set_initial_length(value);
            }
            0xFF1C => {
                self.volume = match value & 0b0110_0000 {
                    0 => WaveChannelVolume::Mute,
                    1 => WaveChannelVolume::Volume100,
                    2 => WaveChannelVolume::Volume50,
                    _ => WaveChannelVolume::Volume25,
                };
            }
            0xFF1D => {
                self.period.set_period_lower(value);
            }
            0xFF1E => {
                self.period.set_period_upper(value & 0b0000_0111);
                self.length_timer.set_enabled(value & 0b0100_0000 != 0);
                if value & 0b1000_0000 != 0 {
                    self.enabled = true;
                    self.length_timer.reset();
                    self.period.reset();
                    self.period.reset_phase_counter();
                }
            }
            0xFF30..=0xFF3F => {
                if !self.enabled {
                    self.wave_ram[address as usize - 0xFF30] = value;
                }
            }
            _ => {}
        }
    }

    fn tick(&mut self, cycles: u32) {
        self.clock += cycles;

        if self.length_timer.tick(cycles) {
            self.enabled = false;
            self.current_amplitude = 0;
        }

        if let Some(PhaseUpdate(phase)) = self.period.tick(cycles) {
            let wave_ram_index = phase / 2;
            let wave_value = match phase % 2 {
                0 => (self.wave_ram[wave_ram_index as usize] & 0xF0) >> 4,
                _ => self.wave_ram[wave_ram_index as usize] & 0x0F,
            };
            let amplitude = match self.volume {
                WaveChannelVolume::Mute => 0,
                WaveChannelVolume::Volume100 => wave_value,
                WaveChannelVolume::Volume50 => wave_value >> 1,
                WaveChannelVolume::Volume25 => wave_value >> 2,
            };
            self.current_amplitude = amplitude;
        }

        if self.current_amplitude != self.last_amplitude {
            let delta =
                ((self.current_amplitude as i32) - (self.last_amplitude as i32)) * (32767 / 15);
            self.blip.add_delta(self.clock, delta);
            self.last_amplitude = self.current_amplitude;
        }
    }
}

impl AudioChannel for WaveChannel {
    fn get_samples(&mut self) -> [i16; SAMPLE_BUFFER_SIZE] {
        self.blip.end_frame(self.clock);
        self.clock = 0;

        let mut buffer = [0; SAMPLE_BUFFER_SIZE];

        self.blip.read_samples(&mut buffer, false);

        buffer
    }

    #[inline]
    fn is_enabled(&self) -> bool {
        self.enabled
    }
}

enum WaveChannelVolume {
    Mute,
    Volume100,
    Volume50,
    Volume25,
}

pub struct NoiseChannel {
    enabled: bool,
    length_timer: LengthTimer,
    volume_envelope: VolumeEnvelope,
    current_amplitude: u8,
    last_amplitude: u8,
    clock: u32,
    lsfr: Lsfr,
    blip: BlipBuf,
}

impl NoiseChannel {
    pub fn new() -> Self {
        let mut blip = BlipBuf::new(2048);
        blip.set_rates(
            GlobalConstants::SYSTEM_CLOCK_RATE as f64,
            SAMPLE_RATE as f64,
        );
        NoiseChannel {
            enabled: false,
            length_timer: LengthTimer::new(64),
            volume_envelope: VolumeEnvelope::new(),
            current_amplitude: 0,
            last_amplitude: 0,
            clock: 0,
            lsfr: Lsfr::new(),
            blip,
        }
    }
}

impl Default for NoiseChannel {
    fn default() -> Self {
        Self::new()
    }
}

impl Register for NoiseChannel {
    fn read(&self, address: u16) -> u8 {
        match address {
            0xFF21 => (&self.volume_envelope).into(),
            0xFF22 => (&self.lsfr).into(),
            0xFF23 => match self.length_timer.is_enabled() {
                true => 0b0100_0000,
                false => 0,
            },
            _ => 0xFF,
        }
    }

    fn write(&mut self, address: u16, value: u8) {
        match address {
            0xFF20 => {
                self.length_timer.set_initial_length(value & 0b0011_1111);
            }
            0xFF21 => {
                self.volume_envelope.update(value);

                if self.volume_envelope.is_disabled() {
                    self.enabled = false;
                }
            }
            0xFF22 => {
                self.lsfr.update(value);
            }
            0xFF23 => {
                if value & 0b1000_0000 != 0 {
                    self.enabled = true;
                    self.length_timer.reset();
                    self.volume_envelope.reset();
                    self.lsfr.reset();
                }

                self.length_timer.set_enabled(value & 0b0100_0000 != 0);
            }
            _ => {}
        }
    }

    fn tick(&mut self, cycles: u32) {
        self.clock += cycles;

        if self.length_timer.tick(cycles) {
            self.enabled = false;
            self.current_amplitude = 0;
        }

        if let Some(VolumeUpdate(volume)) = self.volume_envelope.tick(cycles) {
            match self.lsfr.is_high() {
                true => self.current_amplitude = volume,
                false => self.current_amplitude = 0,
            }
        }

        if let Some(LSFRUpdate(high)) = self.lsfr.tick(cycles) {
            match high {
                true => self.current_amplitude = self.volume_envelope.get_volume(),
                false => self.current_amplitude = 0,
            }
        }

        if self.current_amplitude != self.last_amplitude {
            let delta =
                ((self.current_amplitude as i32) - (self.last_amplitude as i32)) * (32767 / 15);
            self.blip.add_delta(self.clock, delta);
            self.last_amplitude = self.current_amplitude;
        }
    }
}

impl AudioChannel for NoiseChannel {
    fn get_samples(&mut self) -> [i16; SAMPLE_BUFFER_SIZE] {
        self.blip.end_frame(self.clock);
        self.clock = 0;

        let mut buffer = [0; SAMPLE_BUFFER_SIZE];

        self.blip.read_samples(&mut buffer, false);

        buffer
    }

    #[inline]
    fn is_enabled(&self) -> bool {
        self.enabled
    }
}

#[repr(transparent)]
struct LSFRUpdate(bool);

#[derive(Debug, PartialEq, Clone, Copy)]
enum LSFRWidth {
    Bit15,
    Bit7,
}

struct Lsfr {
    value: u16,
    width: LSFRWidth,
    period: u32,
    clock: u32,
    register: u8,
}

impl Lsfr {
    fn new() -> Self {
        Lsfr {
            value: 0,
            width: LSFRWidth::Bit15,
            period: 0,
            clock: 0,
            register: 0,
        }
    }

    fn update(&mut self, value: u8) {
        self.width = match value & 0b0000_1000 {
            0 => LSFRWidth::Bit15,
            _ => LSFRWidth::Bit7,
        };
        let shift = ((value & 0xF0) >> 4) as u32;
        self.period = (((value as u32) & 0b111) * 16).max(8) << shift;

        self.register = value;
    }

    fn tick(&mut self, cycles: u32) -> Option<LSFRUpdate> {
        self.clock += cycles;

        if self.clock < self.period {
            return None;
        }

        self.clock -= self.period;

        let bit1 = self.value & 1;
        let bit2 = (self.value & 2) >> 1;

        self.value &= !0x8000;
        if bit1 == bit2 {
            self.value |= 0x8000;
            if self.width == LSFRWidth::Bit7 {
                self.value |= 0x80;
            }
        }

        self.value >>= 1;

        if self.value & 1 != bit1 {
            return Some(LSFRUpdate(self.value & 1 == 1));
        }

        None
    }

    fn is_high(&self) -> bool {
        self.value & 1 != 0
    }

    fn reset(&mut self) {
        self.value = 0
    }
}

impl From<&Lsfr> for u8 {
    fn from(value: &Lsfr) -> Self {
        value.register
    }
}

pub struct LengthTimer {
    enabled: bool,
    initial_length_timer: u8,
    current_length_timer: u16,
    target_length: u16,
    accumulated_cycles: u32,
}

impl LengthTimer {
    pub fn new(target_length: u16) -> Self {
        LengthTimer {
            enabled: false,
            initial_length_timer: 0,
            current_length_timer: 0,
            target_length,
            accumulated_cycles: 0,
        }
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub fn set_initial_length(&mut self, value: u8) {
        self.initial_length_timer = value;
    }

    pub fn get_initial_length(&self) -> u8 {
        self.initial_length_timer
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn reset(&mut self) {
        self.current_length_timer = self.initial_length_timer as u16;
    }

    pub fn tick(&mut self, cycles: u32) -> bool {
        if !self.enabled {
            return false;
        }

        self.accumulated_cycles += cycles;
        if self.accumulated_cycles < 16384 {
            return false;
        }
        self.accumulated_cycles -= 16384;
        self.current_length_timer = self.current_length_timer.saturating_add(1);

        if self.current_length_timer == self.target_length {
            self.enabled = false;
            return true;
        }

        false
    }
}

pub struct Period {
    period_divider: u16,
    period_reset: u16,
    period_phase: u8,
    clock_rate: u16,
    phase_count: u8,
}

#[repr(transparent)]
pub struct PhaseUpdate(pub u8);

impl Period {
    pub fn new(clock_rate: u16, phase_count: u8) -> Self {
        Period {
            period_divider: 0,
            period_reset: 0,
            period_phase: 0,
            clock_rate,
            phase_count,
        }
    }

    pub fn tick(&mut self, cycles: u32) -> Option<PhaseUpdate> {
        if self.period_divider == 0 {
            self.reset();
            return Some(PhaseUpdate(self.period_phase));
        }

        self.period_divider = self.period_divider.saturating_sub(cycles as u16);

        None
    }

    pub fn reset(&mut self) {
        self.period_divider = (2048 - self.period_reset.min(2048)) * self.clock_rate;
        self.period_phase = (self.period_phase + 1) % self.phase_count;
    }

    pub fn reset_phase_counter(&mut self) {
        self.period_phase = 0;
    }

    pub fn get_phase(&self) -> u8 {
        self.period_phase
    }

    pub fn set_period(&mut self, value: u16) {
        self.period_reset = value;
    }

    pub fn get_period(&self) -> u16 {
        self.period_reset
    }

    pub fn set_period_lower(&mut self, value: u8) {
        self.period_reset = (self.period_reset & 0xFF00) | value as u16;
    }

    pub fn set_period_upper(&mut self, value: u8) {
        self.period_reset = (self.period_reset & 0x00FF) | (((value as u16) & 0b0000_0111) << 8);
    }
}

use crate::gameboy::{memory::Register, GlobalConstants};

use super::{SAMPLE_BUFFER_SIZE, SAMPLE_RATE};

#[derive(Debug, PartialEq, Clone)]
pub struct Sweep {
    pace: u8,
    direction: SweepDirection,
    step: u8,
    accumulated_cycles: u32,
}

impl Sweep {
    pub fn tick(&mut self, cycles: u32, period_value: u16) -> u16 {
        self.accumulated_cycles += cycles;
        let increment = GlobalConstants::SYSTEM_CLOCK_RATE / 128 * (self.pace as u32);
        if increment == 0 || self.accumulated_cycles < increment {
            return period_value;
        }
        self.accumulated_cycles -= increment;

        match self.direction {
            SweepDirection::Increase => period_value + (period_value >> self.step),
            SweepDirection::Decrease => period_value - (period_value >> self.step),
        }
    }

    pub fn disable(&mut self) {
        self.pace = 0;
    }
}

impl From<u8> for Sweep {
    fn from(value: u8) -> Self {
        let pace = (value & 0b0111_0000) >> 4;
        let direction = match value & 0b0000_1000 == 0 {
            true => SweepDirection::Increase,
            false => SweepDirection::Decrease,
        };
        let step = value & 0b0000_0111;

        Sweep {
            pace,
            direction,
            step,
            accumulated_cycles: 0,
        }
    }
}

impl From<&Sweep> for u8 {
    fn from(value: &Sweep) -> Self {
        ((value.pace & 0b0000_0111) << 4)
            | match value.direction {
                SweepDirection::Increase => 0,
                SweepDirection::Decrease => 0b0000_1000,
            }
            | (value.step & 0b0000_0111)
    }
}

#[derive(Debug, PartialEq, Clone)]
enum SweepDirection {
    Increase,
    Decrease,
}

pub struct VolumeEnvelope {
    current_volume: u8,
    initial_volume: u8,
    envelope_direction: VolumeEnvelopeDirection,
    envelope_sweep_pace: u8,
    envelope_timer: u8,
    accumulated_cycles: u32,
}

pub struct VolumeUpdate(pub u8);

impl VolumeEnvelope {
    pub fn new() -> Self {
        VolumeEnvelope {
            current_volume: 0,
            initial_volume: 0,
            envelope_direction: VolumeEnvelopeDirection::Decreasing,
            envelope_sweep_pace: 0,
            envelope_timer: 0,
            accumulated_cycles: 0,
        }
    }

    pub fn tick(&mut self, cycles: u32) -> Option<VolumeUpdate> {
        self.accumulated_cycles += cycles;
        if self.accumulated_cycles < 65536 {
            return None;
        }
        self.accumulated_cycles -= 65536;

        if self.envelope_sweep_pace == 0 {
            // Setting this to 0 disables the envelope.
            return None;
        }

        self.envelope_timer += 1;
        if self.envelope_timer >= self.envelope_sweep_pace {
            self.envelope_timer = 0;

            match self.envelope_direction {
                VolumeEnvelopeDirection::Increasing => {
                    if self.current_volume < 0xF {
                        self.current_volume += 1;
                        return Some(VolumeUpdate(self.current_volume));
                    }
                }
                VolumeEnvelopeDirection::Decreasing => {
                    if self.current_volume > 0 {
                        self.current_volume -= 1;
                        return Some(VolumeUpdate(self.current_volume));
                    }
                }
            }
        }

        None
    }

    pub fn update(&mut self, value: u8) {
        self.initial_volume = value >> 4;
        self.envelope_direction = match value & 0b0000_1000 == 0 {
            true => VolumeEnvelopeDirection::Decreasing,
            false => VolumeEnvelopeDirection::Increasing,
        };
        self.envelope_sweep_pace = value & 0b0000_0111;
    }

    pub fn is_disabled(&self) -> bool {
        if self.initial_volume == 0
            && self.envelope_direction == VolumeEnvelopeDirection::Decreasing
        {
            return true;
        }
        false
    }

    pub fn get_volume(&self) -> u8 {
        self.current_volume
    }

    pub fn get_volume_f32(&self) -> f32 {
        self.current_volume as f32 / 15.0
    }

    pub fn reset(&mut self) {
        self.current_volume = self.initial_volume;
        self.envelope_timer = 0;
    }
}

impl Default for VolumeEnvelope {
    fn default() -> Self {
        Self::new()
    }
}

impl From<&VolumeEnvelope> for u8 {
    fn from(value: &VolumeEnvelope) -> Self {
        (value.initial_volume << 4)
            | match value.envelope_direction {
                VolumeEnvelopeDirection::Decreasing => 0,
                VolumeEnvelopeDirection::Increasing => 0b0000_1000,
            }
            | value.envelope_sweep_pace
    }
}

#[derive(Debug, PartialEq, Clone)]
enum VolumeEnvelopeDirection {
    Increasing,
    Decreasing,
}
