use bitflags::bitflags;
use log::trace;
use std::collections::VecDeque;

use crate::gameboy::display::Color;

use super::{
    display::Display,
    memory::{Interrupt, Register},
};

const SCANLINE_CYCLES: u32 = 456;

const LCDC_ADDRESS: u16 = 0xFF40;
const STAT_ADDRESS: u16 = 0xFF41;
const SCY_ADDRESS: u16 = 0xFF42;
const SCX_ADDRESS: u16 = 0xFF43;
const LY_ADDRESS: u16 = 0xFF44;
const LYC_ADDRESS: u16 = 0xFF45;
const BGP_ADDRESS: u16 = 0xFF47;
const OBP0_ADDRESS: u16 = 0xFF48;
const OBP1_ADDRESS: u16 = 0xFF49;
const WY_ADDRESS: u16 = 0xFF4A;
const WX_ADDRESS: u16 = 0xFF4B;

pub struct PPU {
    lcd_state: LcdState,
    pending_interrupts: Option<Interrupt>,
    vram: [u8; 8192],
    oam: [u8; 160],
    clock: u32,
    scanline_clock: u32,
    display: Box<dyn Display>,
    operation_queue: VecDeque<Operation>,
    mode: Mode,
    sprite_buffer: Vec<Sprite>,
    bg_fifo: VecDeque<Pixel>,
    sprite_fifo: VecDeque<Pixel>,
    lx: u8,
    wlc: u8,
    window_mode: bool,
    active_interrupts: STAT,
    registers: InternalRegisters,
}

impl PPU {
    pub fn new(display: Box<dyn Display>) -> Self {
        let mut operation_queue = VecDeque::new();
        operation_queue.push_back(Operation::NewFrame);

        PPU {
            lcd_state: LcdState::Disabled,
            pending_interrupts: None,
            vram: [0; 8192],
            oam: [0; 160],
            clock: 0,
            scanline_clock: 0,
            display,
            operation_queue,
            mode: Mode::OAMScan,
            sprite_buffer: Vec::new(),
            bg_fifo: VecDeque::new(),
            sprite_fifo: VecDeque::new(),
            lx: 0,
            wlc: 0,
            window_mode: false,
            active_interrupts: STAT::empty(),
            registers: InternalRegisters::new(),
        }
    }

    fn fetch_sprite(&self, address: u16) -> Sprite {
        debug_assert!(
            address >= 0xFE00 && address + 3 <= 0xFE9F,
            "Provided OAM base address {:#05x} exceeds bounds of OAM memory.",
            address
        );
        debug_assert!(
            (address - 0xFE00) % 4 == 0,
            "Provided OAM base address {:#05x} does not align with 4-byte OAM data.",
            address
        );

        let y = self.read(address);
        let x = self.read(address + 1);
        let tile_index = self.read(address + 2);
        let attributes = SpriteAttributes::from_bits_truncate(self.read(address + 3));

        Sprite {
            y,
            x,
            tile_index,
            attributes,
        }
    }

    fn enabled(&self) -> bool {
        self.registers.lcdc.contains(LCDC::LCD_DISPLAY_ENABLE)
    }

    fn sprite_height(&self) -> u8 {
        match self.registers.lcdc {
            lcdc if lcdc.contains(LCDC::SPRITE_DOUBLE_SIZE) => 16,
            _ => 8,
        }
    }

    fn get_bg_tile_number(&self) -> u8 {
        let tilemap_base_address: u16 = match self.registers.lcdc.contains(LCDC::BG_TILEMAP_SELECT)
        {
            true => 0x9C00,
            false => 0x9800,
        };

        let x_coordinate = (((self.registers.scx as u16) + (self.lx as u16)) / 8) & 0x1F;
        let y_coordinate =
            ((((self.registers.ly as u16) + (self.registers.scy as u16)) / 8) & 0x1F) << 5;

        // let tilemap_offset = (x_coordinate + 32 * (y_coordinate / 8)) & 0x3FF;
        let tilemap_address = tilemap_base_address + x_coordinate + y_coordinate;

        self.read(tilemap_address)
    }

    fn fetch_bg_tile(&self, tile_number: u8) -> (u8, u8) {
        let mut base_address = 0x8000;

        if tile_number & 0x80 == 0 && !self.registers.lcdc.contains(LCDC::TILE_ADDRESSING_MODE) {
            base_address |= 0x1000;
        }

        let tile_offset = (tile_number as u16) << 4;
        let y_offset =
            (((self.registers.ly as u16).wrapping_add(self.registers.scy as u16)) % 8) << 1;

        let tile_address = base_address + tile_offset + y_offset;

        debug_assert!(
            match self.registers.lcdc.contains(LCDC::TILE_ADDRESSING_MODE) {
                true => (0x8000..=0x8FFF).contains(&tile_address),
                false => (0x8800..=0x97FF).contains(&tile_address),
            },
            "Background tile address out of range: {}.",
            tile_address
        );

        let low = self.read(tile_address);
        let high = self.read(tile_address.wrapping_add(1));

        (low, high)
    }

    fn get_window_tile_number(&self) -> u8 {
        let tilemap_base_address: u16 =
            match self.registers.lcdc.contains(LCDC::WINDOW_TILEMAP_SELECT) {
                true => 0x9C00,
                false => 0x9800,
            };

        let window_x = ((self.lx as u16) + 7).wrapping_sub(self.registers.wx as u16);

        let x_coordinate = (window_x / 8) & 0x1F;
        let y_coordinate = (((self.wlc as u16) / 8) & 0x1F) << 5;

        let tilemap_address = x_coordinate | y_coordinate | tilemap_base_address;

        self.read(tilemap_address)
    }

    fn fetch_window_tile(&self, tile_number: u8) -> (u8, u8) {
        let mut base_address = 0x8000;

        if tile_number & 0x80 == 0 && !self.registers.lcdc.contains(LCDC::TILE_ADDRESSING_MODE) {
            base_address |= 0x1000;
        }

        let tile_offset = (tile_number as u16) << 4;
        let y_offset = ((self.wlc as u16) % 8) << 1;

        let tile_address = base_address + tile_offset + y_offset;

        debug_assert!(
            match self.registers.lcdc.contains(LCDC::TILE_ADDRESSING_MODE) {
                true => (0x8000..=0x8FFF).contains(&tile_address),
                false => (0x8800..=0x97FF).contains(&tile_address),
            },
            "Window tile address out of range: {}.",
            tile_address
        );

        let low = self.read(tile_address);
        let high = self.read(tile_address.wrapping_add(1));

        (low, high)
    }

    fn fetch_sprite_tile(&self, sprite: Sprite) -> (u8, u8) {
        let flipped = sprite.attributes.contains(SpriteAttributes::Y_FLIP);
        let height = self.sprite_height() as u16;

        debug_assert!(
            self.registers.ly + 16 >= sprite.y
                && self.registers.ly + 16 - (height as u8) < sprite.y,
            "Attempted to fetch tile for a sprite not on this scanline. LY: {} SY: {}",
            self.registers.ly,
            sprite.y
        );

        let ly_offset = ((self.registers.ly as u16) + 16).wrapping_sub(sprite.y as u16);

        let tile_index = match height {
            16 => sprite.tile_index & !1,
            _ => sprite.tile_index,
        };

        let mem_offset = match flipped {
            true => 2 * (height - ly_offset - 1),
            false => 2 * ly_offset,
        };

        let tile_address = 0x8000 + (tile_index as u16 * 16) + mem_offset;

        let mut low = self.read(tile_address);
        let mut high = self.read(tile_address.wrapping_add(1));

        if sprite.attributes.contains(SpriteAttributes::X_FLIP) {
            low = low.reverse_bits();
            high = high.reverse_bits();
        }

        (low, high)
    }

    fn generate_bg_pixels(&self, low: u8, high: u8) -> [Pixel; 8] {
        let mut pixels = [Pixel::default(); 8];
        for (i, pixel) in pixels.iter_mut().enumerate() {
            let low_bit = (low >> (7 - i)) & 1;
            let high_bit = (high >> (7 - i)) & 1;

            let color = (high_bit << 1) | low_bit;

            pixel.color = color;
            pixel.palette = Palette::Bgp;
        }

        pixels
    }

    fn generate_obj_pixels(&self, low: u8, high: u8, sprite: Sprite) -> [Pixel; 8] {
        let mut pixels = [Pixel::default(); 8];
        for (i, pixel) in pixels.iter_mut().enumerate() {
            let low_bit = (low >> (7 - i)) & 1;
            let high_bit = (high >> (7 - i)) & 1;

            let color = (high_bit << 1) | low_bit;

            pixel.color = color;
            pixel.priority = sprite.attributes.contains(SpriteAttributes::PRIORITY);
            pixel.palette = match sprite.attributes.contains(SpriteAttributes::DMG_PALETTE) {
                false => Palette::Obp0,
                true => Palette::Obp1,
            };
        }

        pixels
    }

    fn enable_stat_interrupt(&mut self, interrupt: STAT) {
        // The stat line is the logical OR between all enabled stat sources and respective states
        let current_stat_line = !(self.active_interrupts & self.registers.stat).is_empty();

        self.active_interrupts.insert(interrupt);

        let modified_stat_line = !(self.active_interrupts & self.registers.stat).is_empty();

        // Rising edge detection
        if let (false, true) = (current_stat_line, modified_stat_line) {
            self.pending_interrupts
                .get_or_insert(Interrupt::empty())
                .insert(Interrupt::LCD);
        }
    }

    fn disable_stat_interrupt(&mut self, interrupt: STAT) {
        self.active_interrupts.remove(interrupt);
    }

    fn check_lyc(&mut self) {
        if self.registers.ly == self.registers.lyc {
            self.registers.stat.insert(STAT::COINCIDENCE_FLAG);
            self.enable_stat_interrupt(STAT::LYC_INTERRUPT_ENABLE);
        } else {
            self.registers.stat.remove(STAT::COINCIDENCE_FLAG);
            self.disable_stat_interrupt(STAT::LYC_INTERRUPT_ENABLE);
        }
    }

    fn reset_scanline(&mut self, scanline: u8) {
        // Update LY and reset LX
        self.registers.ly = scanline;
        self.lx = 0;

        self.check_lyc();

        // Increment the window line counter if we rendered any window pixels in the last scanline
        if self.window_mode {
            self.wlc += 1;
        }
        self.window_mode = false;

        // Reset the clock
        self.scanline_clock = 0;

        // Clear all buffers
        self.sprite_buffer.clear();
        self.sprite_fifo.clear();
        self.bg_fifo.clear();
    }
}

impl Register for PPU {
    fn read(&self, address: u16) -> u8 {
        match address {
            0xFF40..=0xFF4B => self.registers.read(address),
            0x8000..=0x9FFF => self.vram[(address - 0x8000) as usize],
            0xFE00..=0xFE9F => self.oam[(address - 0xFE00) as usize],
            _ => 0xFF,
        }
    }

    fn write(&mut self, address: u16, value: u8) {
        match address {
            0xFF41 => {
                // The stat line is the logical OR between all enabled stat sources and respective states
                let current_stat_line = !(self.active_interrupts & self.registers.stat).is_empty();

                self.registers.write(address, value);

                let modified_stat_line = !(self.active_interrupts & self.registers.stat).is_empty();

                // Rising edge detection
                if let (false, true) = (current_stat_line, modified_stat_line) {
                    self.pending_interrupts
                        .get_or_insert(Interrupt::empty())
                        .insert(Interrupt::LCD);
                }
            }
            0xFF45 => {
                self.registers.write(address, value);
                if self.lcd_state == LcdState::Enabled {
                    self.check_lyc();
                }
            }
            0xFF40..=0xFF4B => self.registers.write(address, value),
            0x8000..=0x9FFF => self.vram[(address - 0x8000) as usize] = value,
            0xFE00..=0xFE9F => self.oam[(address - 0xFE00) as usize] = value,
            _ => {}
        }
    }

    fn retrieve_interrupts(&mut self) -> Option<Interrupt> {
        self.pending_interrupts.take()
    }

    fn tick(&mut self, cycles: u32) {
        match self.lcd_state {
            LcdState::Enabled => {
                if !self.enabled() {
                    self.mode = Mode::HBlank;
                    self.registers.stat.remove(STAT::MODE_0 | STAT::MODE_1);
                    self.disable_stat_interrupt(
                        STAT::OAM_SCAN_INTERRUPT_ENABLE | STAT::VBLANK_INTERRUPT_ENABLE,
                    );

                    self.operation_queue.clear();

                    self.lcd_state = LcdState::Disabled;
                    trace!("PPU disabled.");

                    return;
                }
            }
            LcdState::Disabled => {
                if self.enabled() {
                    self.operation_queue.clear();
                    self.operation_queue.extend([ModeChange(Drawing), Sleep(2)]);
                    self.lcd_state = LcdState::Enabled;
                    self.registers.ly = 0;
                    self.check_lyc();
                    trace!("PPU enabled.");
                }
                return;
            }
        }

        self.clock += cycles;

        debug_assert!(
            !self.operation_queue.is_empty(),
            "PPU operation queue was empty."
        );

        use Mode::*;
        use Operation::*;

        while self
            .operation_queue
            .front()
            .expect("PPU operation queue was empty.")
            .cycles()
            < self.clock
        {
            let operation = self.operation_queue.pop_front().unwrap();
            self.clock -= operation.cycles();
            self.scanline_clock += operation.cycles();

            match operation {
                NewFrame => {
                    self.wlc = 0;
                    self.reset_scanline(0);

                    // This state always preceeds OAMScan mode
                    self.operation_queue.push_back(ModeChange(OAMScan))
                }
                NewScanline => {
                    self.reset_scanline(self.registers.ly + 1);

                    #[cfg(debug_assertions)]
                    self.display.render_debug_box(
                        (self.registers.wx.max(7) - 7).min(160),
                        self.registers.wy.min(144),
                        160,
                        144,
                        Color(255, 0, 0, 50),
                    );

                    // This state can either preceed OAMScan mode for lines 1-143, or it can
                    // trigger a mode change to VBlank on line 144, or a new frame on line 154
                    match self.registers.ly {
                        0..144 => self.operation_queue.push_back(ModeChange(OAMScan)),
                        144 => self.operation_queue.push_back(ModeChange(VBlank)),
                        145..154 => self
                            .operation_queue
                            .extend([Sleep(SCANLINE_CYCLES), NewScanline]),
                        _ => self.operation_queue.push_back(NewFrame),
                    }
                }
                ModeChange(mode) => {
                    // Consecutive modes enabled in STAT will result in the later mode not
                    // triggering an interrupt as the stat line never goes low. This is emulated by
                    // enabling the next mode before disabling the previous.
                    self.enable_stat_interrupt(match mode {
                        HBlank => STAT::HBLANK_INTERRUPT_ENABLE,
                        // If bit 5 (mode 2 OAM interrupt) is set, an interrupt is also triggered at
                        // line 144 when vblank starts.
                        VBlank => STAT::VBLANK_INTERRUPT_ENABLE | STAT::OAM_SCAN_INTERRUPT_ENABLE,
                        OAMScan => STAT::OAM_SCAN_INTERRUPT_ENABLE,
                        Drawing => STAT::empty(),
                    });

                    self.disable_stat_interrupt(match self.mode {
                        HBlank => STAT::HBLANK_INTERRUPT_ENABLE,
                        VBlank => STAT::VBLANK_INTERRUPT_ENABLE,
                        OAMScan => STAT::OAM_SCAN_INTERRUPT_ENABLE,
                        Drawing => STAT::empty(),
                    });

                    // Update the two LSBs of STAT for the new mode
                    match mode {
                        HBlank => self.registers.stat.remove(STAT::MODE_0 | STAT::MODE_1),
                        VBlank => {
                            self.registers.stat.remove(STAT::MODE_1);
                            self.registers.stat.insert(STAT::MODE_0);
                        }
                        OAMScan => {
                            self.registers.stat.remove(STAT::MODE_0);
                            self.registers.stat.insert(STAT::MODE_1);
                        }
                        Drawing => {
                            self.registers.stat.insert(STAT::MODE_0 | STAT::MODE_1);
                        }
                    }

                    self.mode = mode;

                    match mode {
                        OAMScan => {
                            for i in 0..40 {
                                self.operation_queue.push_back(OAMSearch(i as u16));
                            }
                            self.operation_queue.push_back(ModeChange(Drawing));
                        }
                        Drawing => {
                            // Adds a 12 cycle sleep to simulate the initial FIFO load
                            self.operation_queue.extend([Sleep(12), CheckPixel]);
                        }
                        HBlank => {
                            debug_assert!(
                                (172..369).contains(&self.scanline_clock),
                                "Invalid number of cycles for drawing phase exceeded. Expected 172-369, Actual {}.",
                                self.scanline_clock
                            );

                            let remaining_cycles = SCANLINE_CYCLES - self.scanline_clock;

                            self.operation_queue.clear();
                            self.operation_queue.push_back(Sleep(remaining_cycles));
                            self.operation_queue.push_back(NewScanline);
                        }
                        VBlank => {
                            self.display.present();

                            self.pending_interrupts
                                .get_or_insert(Interrupt::empty())
                                .insert(Interrupt::VBLANK);

                            self.operation_queue.clear();
                            self.operation_queue
                                .extend([Sleep(SCANLINE_CYCLES), NewScanline]);
                        }
                    }
                }
                OAMSearch(index) => {
                    debug_assert!(
                        index < 40,
                        "The provided OAM search index {} exceeds the maximum number of OAM objects (40).",
                        index
                    );

                    if self.sprite_buffer.len() >= 10 {
                        continue;
                    }

                    let base_address = (index * 4) + 0xFE00;
                    let sprite = self.fetch_sprite(base_address);

                    if self.registers.ly + 16 >= sprite.y
                        && self.registers.ly + 16 < sprite.y + self.sprite_height()
                    {
                        self.sprite_buffer.push(sprite);
                    }
                }
                FetchBackgroundPixels => {
                    let tile_number = self.get_bg_tile_number();
                    let (tile_low, tile_high) = self.fetch_bg_tile(tile_number);
                    let pixels = self.generate_bg_pixels(tile_low, tile_high);

                    self.operation_queue.push_back(PushBackgroundPixels(pixels));
                }
                FetchWindowPixels => {
                    let tile_number = self.get_window_tile_number();
                    let (tile_low, tile_high) = self.fetch_window_tile(tile_number);
                    let pixels = self.generate_bg_pixels(tile_low, tile_high);

                    self.operation_queue.push_back(PushBackgroundPixels(pixels));
                }
                FetchSpritePixels(sprite) => {
                    let (tile_low, tile_high) = self.fetch_sprite_tile(sprite);

                    #[cfg(debug_assertions)]
                    self.display.render_debug_box(
                        sprite.x.max(8) - 8,
                        sprite.y.max(16) - 16,
                        sprite.x,
                        sprite.y.max(16) - 16 + self.sprite_height(),
                        Color(0, 255, 0, 50),
                    );

                    let pixels = self.generate_obj_pixels(tile_low, tile_high, sprite);

                    for _ in 0..(8 - self.sprite_fifo.len()) {
                        // Fill the buffer up to 8 with transparent pixels
                        self.sprite_fifo.push_back(Pixel::default());
                    }

                    // If the sprite is hanging off the left side of the screen, we only want to
                    // merge the visible pixels with the beginning of the fifo
                    let offscreen_pixels = match sprite.x < 8 {
                        true => 8 - sprite.x,
                        false => 0,
                    } as usize;

                    for (i, pixel) in pixels.iter().enumerate().skip(offscreen_pixels) {
                        // Replace any transparent pixels in the fifo with pixels from this sprite
                        // Otherwise do nothing (this sprite is behind another)
                        if self.sprite_fifo[i - offscreen_pixels].color == 0 {
                            self.sprite_fifo[i - offscreen_pixels] = *pixel;
                        }
                    }

                    // If there are less than 6 remaining pixels in the bg_fifo, there will end up
                    // being a delay equal to 6 minus the length as the bg fetcher restarts
                    if self.bg_fifo.len() < 6 && self.sprite_fifo.is_empty() {
                        self.operation_queue
                            .push_back(Sleep(6 - self.bg_fifo.len() as u32));
                    }
                }
                PushBackgroundPixels(pixels) => {
                    if !self.bg_fifo.is_empty() {
                        debug_assert!(
                            !self.operation_queue.is_empty(),
                            "Invalid state: BG Fifo has pixels left to process but there are no more operations on the queue."
                        );
                        // Cannot push pixels to fifo unless it is empty, try again after a few
                        // more operations
                        self.operation_queue.push_back(PushBackgroundPixels(pixels));
                    }

                    self.bg_fifo.extend(pixels);
                    self.operation_queue.push_back(PopPixels);
                }
                PopPixels => {
                    if self.bg_fifo.is_empty() {
                        debug_assert!(
                            !self.operation_queue.is_empty(),
                            "Unnecessary PopPixel operation queued but there were no other operations pending."
                        );
                        continue;
                    }

                    if self.lx == 0 {
                        debug_assert!(
                            self.bg_fifo.len() == 8,
                            "First call to pop pixels on scanline with {} pixels in bg fifo.",
                            self.bg_fifo.len()
                        );

                        if !self.window_mode {
                            for _ in 0..(self.registers.scx % 8) {
                                self.bg_fifo.pop_front();
                                self.operation_queue.push_back(Sleep(1));
                                // self.sprite_fifo.pop_front();
                            }
                        }
                    }

                    let bg_pixel = self
                        .bg_fifo
                        .pop_front()
                        .expect("Attempted to pop pixel from empty fifo.");

                    let merged_pixel = match self.sprite_fifo.pop_front() {
                        Some(sprite_pixel) if !self.registers.lcdc.contains(LCDC::BGW_ENABLE) => {
                            sprite_pixel
                        }
                        Some(sprite_pixel) if sprite_pixel.color == 0 => bg_pixel,
                        Some(sprite_pixel) if sprite_pixel.priority && bg_pixel.color == 0 => {
                            sprite_pixel
                        }
                        Some(sprite_pixel) if sprite_pixel.priority => bg_pixel,
                        Some(sprite_pixel) => sprite_pixel,
                        None if !self.registers.lcdc.contains(LCDC::BGW_ENABLE) => Pixel::default(),
                        None => bg_pixel,
                    };

                    let palette = match merged_pixel.palette {
                        Palette::Bgp => self.registers.bgp,
                        Palette::Obp0 => self.registers.obp0,
                        Palette::Obp1 => self.registers.obp1,
                    };

                    let color_shift = merged_pixel.color * 2;
                    let color = ((3 << color_shift) & palette) >> color_shift;

                    self.display.push_pixel(self.lx, self.registers.ly, color);

                    // LX Increment
                    self.lx += 1;
                    self.operation_queue.push_back(CheckPixel);
                }
                CheckPixel => {
                    // End of scanline, exit early
                    if self.lx >= 160 {
                        self.operation_queue.push_back(ModeChange(HBlank));
                        continue;
                    }

                    // Check for sprites at this coordinate and start a sprite fetch if so
                    let sprites_enabled = self.registers.lcdc.contains(LCDC::SPRITE_DISPLAY_ENABLE);

                    for sprite in self.sprite_buffer.iter() {
                        if sprite.x.saturating_sub(8) == self.lx && sprites_enabled {
                            self.operation_queue.push_back(FetchSpritePixels(*sprite));
                        }
                    }

                    let window_enabled = self.registers.lcdc.contains(LCDC::WINDOW_DISPLAY_ENABLE);

                    match self.bg_fifo.is_empty() {
                        // BG fifo might still have pixels but we've encountered a window for the first
                        // time this scanline
                        _ if !self.window_mode
                            && window_enabled
                            && self.lx + 7 >= self.registers.wx
                            && self.registers.ly >= self.registers.wy =>
                        {
                            self.window_mode = true;
                            // Empty out the current FIFO and start over from this x coordinate
                            self.bg_fifo.clear();
                            self.operation_queue.clear();

                            // trace!(
                            //     "Entered Window Mode | LY: {} | LX: {} | OpQueue: {:?}",
                            //     self.ly, self.lx, self.operation_queue
                            // );
                            self.operation_queue.push_back(CheckPixel);
                        }
                        // BG Fifo is empty and we are in the window
                        true if self.window_mode => {
                            self.operation_queue.push_back(FetchWindowPixels);
                        }

                        // BG fifo is empty and we are not in the window
                        true => self.operation_queue.push_back(FetchBackgroundPixels),

                        // BG fifo still has pixels, no special cases here just continue popping
                        false => self.operation_queue.push_back(PopPixels),
                    }
                }
                Sleep(_) => {}
            }

            debug_assert!(
                !self.operation_queue.is_empty(),
                "Nothing was pushed to the operation queue following a {:?}.",
                operation
            );
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
enum Mode {
    OAMScan,
    Drawing,
    HBlank,
    VBlank,
}

#[derive(Debug, PartialEq, Clone, Copy)]
enum Operation {
    NewFrame,
    NewScanline,
    ModeChange(Mode),
    OAMSearch(u16),
    FetchBackgroundPixels,
    FetchWindowPixels,
    FetchSpritePixels(Sprite),
    PushBackgroundPixels([Pixel; 8]),
    PopPixels,
    CheckPixel,
    Sleep(u32),
}

impl Operation {
    fn cycles(&self) -> u32 {
        match self {
            Operation::NewFrame => 0,
            Operation::NewScanline => 0,
            Operation::ModeChange(_) => 0,
            Operation::OAMSearch(_) => 2,
            Operation::FetchBackgroundPixels => 8,
            Operation::FetchWindowPixels => 8,
            Operation::FetchSpritePixels(_) => 6,
            Operation::PushBackgroundPixels(_) => 0,
            Operation::PopPixels => 0,
            Operation::CheckPixel => 0,
            Operation::Sleep(cycles) => *cycles,
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
struct Sprite {
    y: u8,
    x: u8,
    tile_index: u8,
    attributes: SpriteAttributes,
}

#[derive(Debug, PartialEq, Clone, Copy)]
struct Pixel {
    color: u8,
    palette: Palette,
    priority: bool,
}

impl Default for Pixel {
    fn default() -> Self {
        Pixel {
            color: 0,
            palette: Palette::Obp0,
            priority: false,
        }
    }
}

/// Represents the different types of palettes a pixel can have
#[derive(Debug, PartialEq, Clone, Copy)]
enum Palette {
    Bgp,
    Obp0,
    Obp1,
}

#[derive(Debug, PartialEq, Clone, Copy)]
enum LcdState {
    Enabled,
    Disabled,
}

bitflags! {

    #[repr(transparent)]
    #[derive(Debug, PartialEq, Clone, Copy)]
    struct SpriteAttributes: u8 {
        /// If set, BG and window colors 1-3 are drawn over this object
        const PRIORITY = 0b1000_0000;

        /// If set, this sprite is flipped vertically
        const Y_FLIP = 0b0100_0000;

        /// If set, this sprite is flipped horizontally
        const X_FLIP = 0b0010_0000;

        /// Selects which OBP palette to use. 0 = OBP0, 1 = OBP1
        const DMG_PALETTE = 0b0001_0000;
    }

    #[repr(transparent)]
    #[derive(Debug, PartialEq, Clone, Copy)]
    struct LCDC: u8 {
        /// Controls whether the PPU should be active at all. 1 = Enabled, 0 = Disabled
        const LCD_DISPLAY_ENABLE = 0b1000_0000;

        /// Controls which tile map is used for the window.
        /// If set to 1, uses 0x9C00..=0x9FFF, otherwise uses 0x9800..=0x9BFF
        const WINDOW_TILEMAP_SELECT = 0b0100_0000;

        /// Enables/disables rendering the window. 1 = Enabled, 0 = Disabled
        const WINDOW_DISPLAY_ENABLE = 0b0010_0000;

        /// Determines which addressing mode to use for tiles. 1 = 8000 Addressing, 0 = 8800
        /// Addressing
        const TILE_ADDRESSING_MODE = 0b0001_0000;

        /// Controls which tile map is used for the background.
        /// If set to 1, uses 0x9C00..=0x9FFF, otherwise uses 0x9800..=0x9BFF
        const BG_TILEMAP_SELECT = 0b0000_1000;

        /// Tall sprite mode uses two tiles for each sprite. 1 = Tall Mode, 0 = Regular Mode
        const SPRITE_DOUBLE_SIZE = 0b0000_0100;

        /// Enables/disables rendering sprites. 1 = Enabled, 0 = Disabled
        const SPRITE_DISPLAY_ENABLE = 0b0000_0010;

        /// Enables/disables drawing both the background and window.
        /// If set to zero, no background or window tiles are drawn.
        /// Note: GBC has different functionality.
        const BGW_ENABLE = 0b0000_0001;
    }

    #[repr(transparent)]
    #[derive(Debug, PartialEq, Clone, Copy)]
    struct STAT: u8 {
        /// Determines whether the LY=LYC triggers an LCD interrupt
        const LYC_INTERRUPT_ENABLE = 0b0100_0000;

        /// Determines whether entering Mode 2 OAM Scan triggers an interrupt
        const OAM_SCAN_INTERRUPT_ENABLE = 0b0010_0000;

        /// Determines whether entering Mode 1 VBlank triggers an interrupt
        const VBLANK_INTERRUPT_ENABLE = 0b0001_0000;

        /// Determines whether entering Mode 0 HBlank triggers an interrupt
        const HBLANK_INTERRUPT_ENABLE = 0b0000_1000;

        /// Indicates whether LY=LYC currently
        const COINCIDENCE_FLAG = 0b0000_0100;

        const MODE_1 = 0b0000_0010;
        const MODE_0 = 0b0000_0001;
    }
}

pub struct InternalRegisters {
    pub ly: u8,
    pub lyc: u8,
    pub wx: u8,
    pub wy: u8,
    pub scx: u8,
    pub scy: u8,
    pub stat: STAT,
    pub lcdc: LCDC,
    pub obp0: u8,
    pub obp1: u8,
    pub bgp: u8,
}

impl InternalRegisters {
    pub fn new() -> Self {
        InternalRegisters {
            ly: 0,
            lyc: 0,
            wx: 0,
            wy: 0,
            scx: 0,
            scy: 0,
            stat: STAT::from_bits_retain(0b1100_0000),
            lcdc: LCDC::empty(),
            obp0: 0,
            obp1: 0,
            bgp: 0,
        }
    }
}

impl Register for InternalRegisters {
    fn read(&self, address: u16) -> u8 {
        match address {
            LY_ADDRESS => self.ly,
            LYC_ADDRESS => self.lyc,
            WY_ADDRESS => self.wy,
            WX_ADDRESS => self.wx,
            SCX_ADDRESS => self.scx,
            SCY_ADDRESS => self.scy,
            STAT_ADDRESS => self.stat.bits(),
            LCDC_ADDRESS => self.lcdc.bits(),
            OBP0_ADDRESS => self.obp0,
            OBP1_ADDRESS => self.obp1,
            BGP_ADDRESS => self.bgp,
            _ => 0xFF,
        }
    }

    fn write(&mut self, address: u16, value: u8) {
        match address {
            LYC_ADDRESS => self.lyc = value,
            WY_ADDRESS => self.wy = value,
            WX_ADDRESS => self.wx = value,
            SCX_ADDRESS => self.scx = value,
            SCY_ADDRESS => self.scy = value,
            STAT_ADDRESS => {
                self.stat = STAT::from_bits_retain(
                    (self.stat.bits() & 0b1100_0111) | (value & 0b0011_1000),
                );
            }
            LCDC_ADDRESS => self.lcdc = LCDC::from_bits_truncate(value),
            OBP0_ADDRESS => self.obp0 = value,
            OBP1_ADDRESS => self.obp1 = value,
            BGP_ADDRESS => self.bgp = value,
            _ => {}
        }
    }
}
