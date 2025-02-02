use arraydeque::{ArrayDeque, Saturating};
use bitflags::bitflags;
use channel::{AudioChannel, NoiseChannel, PulseChannel, WaveChannel};
use crossbeam::channel::{Receiver, Sender};
use rodio::{OutputStream, Sink, Source};

use super::{memory::Register, GlobalConstants};

pub mod channel;

pub const SAMPLE_RATE: u32 = 44100;
pub const AUDIO_FRAME_LENGTH: u32 = 17556;
pub const SAMPLE_BUFFER_SIZE: usize = calculate_sample_buffer_size();

const fn calculate_sample_buffer_size() -> usize {
    let audio_frame_percent =
        (AUDIO_FRAME_LENGTH as f32) / (GlobalConstants::SYSTEM_CLOCK_RATE as f32);
    let sample_buffer_size = SAMPLE_RATE as f32 * audio_frame_percent;
    ceil_f32(sample_buffer_size) as usize
}
const fn ceil_f32(n: f32) -> f32 {
    let int_part = n as i32 as f32;
    if n > int_part {
        int_part + 1.0
    } else {
        int_part
    }
}

pub struct APU {
    enabled: bool,
    channel1: PulseChannel,
    channel2: PulseChannel,
    channel3: WaveChannel,
    channel4: NoiseChannel,
    apu_clock: u32,
    channel_tx: Sender<SampleBuffer>,
    nr51: NR51,
    left_volume: u8,
    right_volume: u8,
}

impl APU {
    pub fn new() -> Self {
        let (channel_tx, channel_rx) = crossbeam::channel::bounded(16);

        let channel1 = PulseChannel::new();
        let channel2 = PulseChannel::new();
        let channel3 = WaveChannel::new();
        let channel4 = NoiseChannel::new();

        let output_channel = OutputChannel::new(SAMPLE_RATE, channel_rx);

        std::thread::spawn(move || {
            let (_stream, stream_handle) = OutputStream::try_default().unwrap();
            let sink = Sink::try_new(&stream_handle).unwrap();

            sink.append(output_channel);

            sink.sleep_until_end();
        });

        APU {
            enabled: false,
            channel1,
            channel2,
            channel3,
            channel4,
            apu_clock: 0,
            channel_tx,
            nr51: NR51::empty(),
            left_volume: 0,
            right_volume: 0,
        }
    }

    fn mix_channels(&mut self) -> [i16; SAMPLE_BUFFER_SIZE] {
        let buffer1 = self.channel1.get_samples();
        let buffer2 = self.channel2.get_samples();
        let buffer3 = self.channel3.get_samples();
        let buffer4 = self.channel4.get_samples();

        // TODO: Add panning or leave as mono?

        let volume_multiplier = (self.left_volume as f32 + self.right_volume as f32 + 2.0) / 16.0;

        let mut final_buffer = [0; SAMPLE_BUFFER_SIZE];
        for i in 0..SAMPLE_BUFFER_SIZE {
            let ch1_sample = buffer1[i] / 4;
            let ch2_sample = buffer2[i] / 4;
            let ch3_sample = buffer3[i] / 4;
            let ch4_sample = buffer4[i] / 4;

            let sum = ch1_sample + ch2_sample + ch3_sample + ch4_sample;
            let scaled_sum = sum as f32 * volume_multiplier;

            final_buffer[i] = scaled_sum as i16;
        }

        final_buffer
    }
}

impl Default for APU {
    fn default() -> Self {
        Self::new()
    }
}

impl Register for APU {
    fn read(&self, address: u16) -> u8 {
        match address {
            0xFF10..=0xFF14 => self.channel1.read(address - 0xFF10),
            0xFF16..=0xFF19 => self.channel2.read(address - 0xFF15),
            0xFF1A..=0xFF1E | 0xFF30..=0xFF3F => self.channel3.read(address),
            0xFF20..=0xFF23 => self.channel4.read(address),
            0xFF24 => ((self.left_volume & 0b111) << 4) | (self.right_volume & 0b111),
            0xFF25 => self.nr51.bits(),
            0xFF26 => {
                ((self.enabled as u8) << 7)
                    | (self.channel1.is_enabled() as u8)
                    | ((self.channel2.is_enabled() as u8) << 1)
                    | ((self.channel3.is_enabled() as u8) << 2)
                    | ((self.channel4.is_enabled() as u8) << 3)
            }
            _ => 0xFF,
        }
    }

    fn write(&mut self, address: u16, value: u8) {
        match address {
            0xFF10..=0xFF14 => self.channel1.write(address - 0xFF10, value),
            0xFF16..=0xFF19 => self.channel2.write(address - 0xFF15, value),
            0xFF1A..=0xFF1E | 0xFF30..=0xFF3F => self.channel3.write(address, value),
            0xFF20..=0xFF23 => self.channel4.write(address, value),
            0xFF24 => {
                self.left_volume = (value >> 4) & 0b111;
                self.right_volume = value & 0b111;
            }
            0xFF25 => self.nr51 = NR51::from_bits_truncate(value),
            0xFF26 => {
                self.enabled = value & 0b1000_0000 != 0;
            }
            _ => {}
        }
    }

    fn tick(&mut self, cycles: u32) {
        if !self.enabled {
            return;
        }

        self.apu_clock += cycles;
        if self.apu_clock >= AUDIO_FRAME_LENGTH {
            let buffer = self.mix_channels();
            // if self.channel_tx.try_send(SampleBuffer(buffer)).is_err() {
            //     warn!("Audio sample block dropped due to full message queue.")
            // }
            self.channel_tx.send(SampleBuffer(buffer)).unwrap();

            self.apu_clock -= AUDIO_FRAME_LENGTH;
        }

        self.channel1.tick(cycles);
        self.channel2.tick(cycles);
        self.channel3.tick(cycles);
        self.channel4.tick(cycles);
    }
}

const CAPACITY: usize = 2048;

pub struct OutputChannel {
    sample_rate: u32,
    channel_rx: Receiver<SampleBuffer>,
    sample_buffer: ArrayDeque<i16, CAPACITY, Saturating>,
    samples_dropped: bool,
}

impl OutputChannel {
    fn new(sample_rate: u32, channel_rx: Receiver<SampleBuffer>) -> Self {
        OutputChannel {
            sample_rate,
            channel_rx,
            sample_buffer: ArrayDeque::new(),
            samples_dropped: false,
        }
    }
}

const CROSSFADE_LENGTH: usize = 64;

impl Iterator for OutputChannel {
    type Item = i16;

    fn next(&mut self) -> Option<Self::Item> {
        if self.sample_buffer.is_empty() {
            if let Ok(SampleBuffer(buffer)) = self.channel_rx.try_recv() {
                self.sample_buffer.extend_back(buffer);
                // self.samples_dropped = self.sample_buffer.is_full();
            }
        }

        let sample = self.sample_buffer.pop_front().unwrap_or(0);

        Some(sample)
    }
}

impl Source for OutputChannel {
    fn current_frame_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> u16 {
        1
    }

    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn total_duration(&self) -> Option<std::time::Duration> {
        None
    }
}

pub struct SampleBuffer([i16; SAMPLE_BUFFER_SIZE]);

bitflags! {
    #[repr(transparent)]
    #[derive(Debug, PartialEq, Clone, Copy)]
    struct NR52: u8 {
        const APU_ENABLE = 0b1000_0000;
        const CH1_ON = 0b0000_0001;
        const CH2_ON = 0b0000_0010;
        const CH3_ON = 0b0000_0100;
        const CH4_ON = 0b0000_1000;
    }

    #[repr(transparent)]
    #[derive(Debug, PartialEq, Clone, Copy)]
    struct NR51: u8 {
        const CH1_RIGHT = 0b0000_0001;
        const CH2_RIGHT = 0b0000_0010;
        const CH3_RIGHT = 0b0000_0100;
        const CH4_RIGHT = 0b0000_1000;
        const CH1_LEFT = 0b0001_0000;
        const CH2_LEFT = 0b0010_0000;
        const CH3_LEFT = 0b0100_0000;
        const CH4_LEFT = 0b1000_0000;
    }
}
