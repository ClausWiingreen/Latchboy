pub mod savefile;

use latchboy_core::{Emulator, FRAMEBUFFER_HEIGHT, FRAMEBUFFER_PIXELS, FRAMEBUFFER_WIDTH};

pub const DMG_PALETTE_ARGB8888: [u32; 4] = [0xFFE0F8D0, 0xFF88C070, 0xFF346856, 0xFF081820];

pub trait FrameSurface {
    fn blit_argb8888(&mut self, width: usize, height: usize, pixels: &[u32]) -> Result<(), String>;
}

#[derive(Debug, Clone)]
pub struct WindowSurface {
    width: usize,
    height: usize,
    pixels: Vec<u32>,
}

impl WindowSurface {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            pixels: vec![0; width * height],
        }
    }

    pub const fn width(&self) -> usize {
        self.width
    }

    pub const fn height(&self) -> usize {
        self.height
    }

    pub fn pixels(&self) -> &[u32] {
        &self.pixels
    }
}

impl FrameSurface for WindowSurface {
    fn blit_argb8888(&mut self, width: usize, height: usize, pixels: &[u32]) -> Result<(), String> {
        if width != self.width || height != self.height {
            return Err(format!(
                "surface size mismatch (surface={}x{}, blit={}x{})",
                self.width, self.height, width, height
            ));
        }
        if pixels.len() != self.pixels.len() {
            return Err(format!(
                "pixel count mismatch (surface={}, blit={})",
                self.pixels.len(),
                pixels.len()
            ));
        }

        self.pixels.copy_from_slice(pixels);
        Ok(())
    }
}

pub fn map_dmg_shades_to_argb8888(
    shades: &[u8; FRAMEBUFFER_PIXELS],
    palette: [u32; 4],
) -> [u32; FRAMEBUFFER_PIXELS] {
    let mut mapped = [0; FRAMEBUFFER_PIXELS];
    for (index, shade) in shades.iter().enumerate() {
        mapped[index] = palette[(*shade).min(3) as usize];
    }
    mapped
}

pub fn present_latest_frame(
    emulator: &Emulator,
    surface: &mut impl FrameSurface,
    palette: [u32; 4],
) -> Result<(), String> {
    let shades = emulator.framebuffer_shades();
    let pixels = map_dmg_shades_to_argb8888(&shades, palette);
    surface.blit_argb8888(FRAMEBUFFER_WIDTH, FRAMEBUFFER_HEIGHT, &pixels)
}
