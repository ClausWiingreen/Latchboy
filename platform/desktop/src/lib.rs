pub mod savefile;

use std::error::Error;
use std::fmt;

use latchboy_core::{Emulator, FRAMEBUFFER_LEN};

/// Stable DMG palette in RGB888 (0x00RRGGBB), darkest shade last.
pub const DMG_PALETTE_RGB: [u32; 4] = [0x00E0F8D0, 0x0088C070, 0x00346856, 0x00081820];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameBlitError {
    FramebufferSizeMismatch { expected: usize, actual: usize },
    SurfaceSizeMismatch { expected: usize, actual: usize },
}

impl fmt::Display for FrameBlitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FramebufferSizeMismatch { expected, actual } => {
                write!(
                    f,
                    "framebuffer length mismatch: expected {expected}, got {actual}"
                )
            }
            Self::SurfaceSizeMismatch { expected, actual } => {
                write!(
                    f,
                    "surface length mismatch: expected {expected}, got {actual}"
                )
            }
        }
    }
}

impl Error for FrameBlitError {}

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

pub trait FramePresenter {
    type Error: Error + Send + Sync + 'static;

    fn is_open(&self) -> bool;
    fn present_frame(&mut self, surface: &[u32]) -> Result<(), Self::Error>;
}

#[derive(Debug)]
pub enum EmulationRunError<E: Error + Send + Sync + 'static> {
    InvalidCycleStep,
    FrameBlit(FrameBlitError),
    Present(E),
}

impl<E: Error + Send + Sync + 'static> fmt::Display for EmulationRunError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidCycleStep => write!(f, "cycle_step must be greater than zero"),
            Self::FrameBlit(error) => write!(f, "{error}"),
            Self::Present(error) => write!(f, "frame presentation failed: {error}"),
        }
    }
}

impl<E: Error + Send + Sync + 'static> Error for EmulationRunError<E> {}

/// Runs a basic emulation loop and presents frames whenever VBlank marks a complete frame.
///
/// Returns the number of frames presented.
pub fn run_emulation_loop<P: FramePresenter>(
    emulator: &mut Emulator,
    presenter: &mut P,
    cycle_step: u32,
    frame_limit: Option<u64>,
    iteration_limit: Option<u64>,
) -> Result<u64, EmulationRunError<P::Error>> {
    if cycle_step == 0 {
        return Err(EmulationRunError::InvalidCycleStep);
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
        if let Some(limit) = iteration_limit {
            if iterations >= limit {
                break;
            }
        }

        emulator.step_cycles(cycle_step);
        iterations += 1;
        if !emulator.take_frame_ready() {
            continue;
        }

        blit_dmg_framebuffer_to_rgb_surface(emulator.framebuffer_pixels(), &mut surface)
            .map_err(EmulationRunError::FrameBlit)?;
        presenter
            .present_frame(&surface)
            .map_err(EmulationRunError::Present)?;
        frames_presented += 1;
    }

    Ok(frames_presented)
}
