pub mod savefile;

use std::error::Error;
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

use latchboy_core::{Emulator, JoypadButton, FRAMEBUFFER_LEN};
use png::{BitDepth, ColorType, Encoder};
use thiserror::Error;

const DMG_FRAME_CYCLES: u32 = 70_224;
// `Emulator::step_cycles` advances by at least the requested cycles and can overshoot by
// up to one CPU instruction. The longest currently implemented instruction is 24 cycles,
// so leave that much headroom to avoid skipping past multiple frame-ready pulses in one step.
const MAX_CPU_INSTRUCTION_CYCLES: u32 = 24;
const MAX_CYCLES_BETWEEN_FRAME_POLLS: u32 = DMG_FRAME_CYCLES - MAX_CPU_INSTRUCTION_CYCLES;

/// Stable DMG palette in RGB888 (0x00RRGGBB), darkest shade last.
pub const DMG_PALETTE_RGB: [u32; 4] = [0x00E0F8D0, 0x0088C070, 0x00346856, 0x00081820];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum FrameBlitError {
    #[error("framebuffer length mismatch: expected {expected}, got {actual}")]
    FramebufferSizeMismatch { expected: usize, actual: usize },
    #[error("surface length mismatch: expected {expected}, got {actual}")]
    SurfaceSizeMismatch { expected: usize, actual: usize },
}

#[derive(Debug, Error)]
pub enum SurfaceImageWriteError {
    #[error("surface length mismatch for {width}x{height}: expected {expected}, got {actual}")]
    SurfaceSizeMismatch {
        width: u32,
        height: u32,
        expected: usize,
        actual: usize,
    },
    #[error("failed to write png at '{path}': {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to encode png at '{path}': {source}")]
    Encode {
        path: String,
        #[source]
        source: png::EncodingError,
    },
}

/// Converts DMG shade-index framebuffer bytes (`0..=3`) into RGB pixels.
pub fn blit_dmg_framebuffer_to_rgb_surface(
    framebuffer: &[u8],
    surface: &mut [u32],
) -> Result<(), FrameBlitError> {
    if framebuffer.len() != FRAMEBUFFER_LEN {
        return Err(FrameBlitError::FramebufferSizeMismatch {
            expected: FRAMEBUFFER_LEN,
            actual: framebuffer.len(),
        });
    }

    if surface.len() != FRAMEBUFFER_LEN {
        return Err(FrameBlitError::SurfaceSizeMismatch {
            expected: FRAMEBUFFER_LEN,
            actual: surface.len(),
        });
    }

    for (dst, &shade) in surface.iter_mut().zip(framebuffer.iter()) {
        let palette_index = usize::from(shade.min(3));
        *dst = DMG_PALETTE_RGB[palette_index];
    }

    Ok(())
}

/// Writes an RGB888 surface (`0x00RRGGBB` pixels) to a PNG image on disk.
pub fn write_rgb_surface_to_png(
    path: &Path,
    surface: &[u32],
    width: u32,
    height: u32,
) -> Result<(), SurfaceImageWriteError> {
    let expected_len = (width as usize).saturating_mul(height as usize);
    if surface.len() != expected_len {
        return Err(SurfaceImageWriteError::SurfaceSizeMismatch {
            width,
            height,
            expected: expected_len,
            actual: surface.len(),
        });
    }

    let file = File::create(path).map_err(|source| SurfaceImageWriteError::Io {
        path: path.display().to_string(),
        source,
    })?;
    let writer = BufWriter::new(file);

    let mut encoder = Encoder::new(writer, width, height);
    encoder.set_color(ColorType::Rgb);
    encoder.set_depth(BitDepth::Eight);
    let mut png_writer =
        encoder
            .write_header()
            .map_err(|source| SurfaceImageWriteError::Encode {
                path: path.display().to_string(),
                source,
            })?;

    let mut pixels = Vec::with_capacity(surface.len().saturating_mul(3));
    for &pixel in surface {
        pixels.push(((pixel >> 16) & 0xFF) as u8);
        pixels.push(((pixel >> 8) & 0xFF) as u8);
        pixels.push((pixel & 0xFF) as u8);
    }

    png_writer
        .write_image_data(&pixels)
        .map_err(|source| SurfaceImageWriteError::Encode {
            path: path.display().to_string(),
            source,
        })?;
    png_writer
        .finish()
        .map_err(|source| SurfaceImageWriteError::Encode {
            path: path.display().to_string(),
            source,
        })?;
    Ok(())
}

pub trait FramePresenter {
    type Error: Error + Send + Sync + 'static;

    fn is_open(&self) -> bool;
    fn poll_events(&mut self) -> Result<(), Self::Error>;
    fn drain_input_events(&mut self) -> Vec<(JoypadButton, bool)> {
        Vec::new()
    }
    fn present_frame(&mut self, surface: &[u32]) -> Result<(), Self::Error>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmulationRunStats {
    pub frames_presented: u64,
    pub iterations: u64,
}

#[derive(Debug, Error)]
pub enum EmulationRunError<E: Error + Send + Sync + 'static> {
    #[error("cycle_step must be greater than zero")]
    InvalidCycleStep,
    #[error("{0}")]
    FrameBlit(FrameBlitError),
    #[error("frame presentation failed: {0}")]
    Present(E),
}

/// Runs a basic emulation loop and presents frames whenever VBlank marks a complete frame.
///
/// Returns the number of frames presented.
pub fn run_emulation_loop_with_stats<P: FramePresenter>(
    emulator: &mut Emulator,
    presenter: &mut P,
    cycle_step: u32,
    frame_limit: Option<u64>,
    iteration_limit: Option<u64>,
) -> Result<EmulationRunStats, EmulationRunError<P::Error>> {
    if cycle_step == 0 {
        return Err(EmulationRunError::InvalidCycleStep);
    }

    fn present_if_ready<P: FramePresenter>(
        emulator: &mut Emulator,
        presenter: &mut P,
        surface: &mut [u32],
    ) -> Result<bool, EmulationRunError<P::Error>> {
        if !emulator.take_frame_ready() {
            return Ok(false);
        }

        blit_dmg_framebuffer_to_rgb_surface(emulator.framebuffer_pixels(), surface)
            .map_err(EmulationRunError::FrameBlit)?;
        presenter
            .present_frame(surface)
            .map_err(EmulationRunError::Present)?;
        Ok(true)
    }

    let mut surface = vec![0u32; FRAMEBUFFER_LEN];
    let mut frames_presented = 0u64;
    let mut iterations = 0u64;

    while presenter.is_open() {
        if let Some(limit) = frame_limit {
            if frames_presented >= limit {
                break;
            }
        }
        presenter
            .poll_events()
            .map_err(EmulationRunError::Present)?;
        for (button, pressed) in presenter.drain_input_events() {
            emulator.set_button_pressed(button, pressed);
        }
        if !presenter.is_open() {
            break;
        }
        if present_if_ready(emulator, presenter, &mut surface)? {
            frames_presented += 1;
            continue;
        }
        let mut cycles_remaining = cycle_step;
        while cycles_remaining != 0 && presenter.is_open() {
            if let Some(limit) = iteration_limit {
                if iterations >= limit {
                    return Ok(EmulationRunStats {
                        frames_presented,
                        iterations,
                    });
                }
            }
            if let Some(limit) = frame_limit {
                if frames_presented >= limit {
                    return Ok(EmulationRunStats {
                        frames_presented,
                        iterations,
                    });
                }
            }
            presenter
                .poll_events()
                .map_err(EmulationRunError::Present)?;
            for (button, pressed) in presenter.drain_input_events() {
                emulator.set_button_pressed(button, pressed);
            }
            if !presenter.is_open() {
                return Ok(EmulationRunStats {
                    frames_presented,
                    iterations,
                });
            }

            if present_if_ready(emulator, presenter, &mut surface)? {
                frames_presented += 1;
                continue;
            }

            let chunk = cycles_remaining.min(MAX_CYCLES_BETWEEN_FRAME_POLLS);
            emulator.step_cycles(chunk);
            cycles_remaining -= chunk;
            iterations += 1;
        }
    }

    Ok(EmulationRunStats {
        frames_presented,
        iterations,
    })
}

/// Backward-compatible convenience wrapper that returns only frame count.
pub fn run_emulation_loop<P: FramePresenter>(
    emulator: &mut Emulator,
    presenter: &mut P,
    cycle_step: u32,
    frame_limit: Option<u64>,
    iteration_limit: Option<u64>,
) -> Result<u64, EmulationRunError<P::Error>> {
    Ok(run_emulation_loop_with_stats(
        emulator,
        presenter,
        cycle_step,
        frame_limit,
        iteration_limit,
    )?
    .frames_presented)
}
