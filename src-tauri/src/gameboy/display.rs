use tauri::{App, AppHandle, Emitter};

/// A Display provides functions to render scanlines and present frames.
pub trait Display {
    // TODO: Color should probably be determined by PPU, not indexed by display

    /// Push a pixel to the current scanline
    fn push_pixel(&mut self, x: u8, y: u8, color: u8);

    /// Presents the rendered frame on the screen. Should be called during VBlank.
    fn present(&mut self);

    /// Renders a debug box around the specified coordinates
    #[cfg(debug_assertions)]
    #[allow(unused_variables)]
    fn render_debug_box(&mut self, nw_x: u8, nw_y: u8, se_x: u8, se_y: u8, color: Color) {}
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub struct Color(pub u8, pub u8, pub u8, pub u8);

pub struct WebviewDisplay {
    app_handle: AppHandle,
    buffer: [[u8; 160]; 144],
}

impl WebviewDisplay {
    pub fn new(app_handle: AppHandle) -> Self {
        WebviewDisplay {
            app_handle,
            buffer: [[0; 160]; 144],
        }
    }
}

impl Display for WebviewDisplay {
    fn push_pixel(&mut self, x: u8, y: u8, color: u8) {
        debug_assert!(
            color <= 3,
            "Invalid color index {} provided to Display.",
            color
        );
        debug_assert!(
            (0..160).contains(&x),
            "Invalid x-coordinate {} provided to Display.",
            x
        );
        debug_assert!(
            (0..144).contains(&y),
            "Invalid y-coordinate {} provided to Display.",
            y
        );

        self.buffer[y as usize][x as usize] = color;
    }

    fn present(&mut self) {
        let flat_buffer: Vec<u8> = self.buffer.iter().flatten().copied().collect();
        self.app_handle
            .emit("gb-present-frame", flat_buffer)
            .unwrap();
    }
}
