use std::env;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use latchboy_core::{
    cartridge::Cartridge, Emulator, FRAMEBUFFER_HEIGHT, FRAMEBUFFER_LEN, FRAMEBUFFER_WIDTH,
};
use latchboy_desktop::savefile::{
    load_save_data_if_available, persist_save_data, save_path_from_rom_path,
    should_persist_after_load,
};
use latchboy_desktop::{run_emulation_loop, write_rgb_surface_to_png, FramePresenter};
use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::pixels::PixelFormatEnum;
use sdl2::render::{Canvas, Texture};
use sdl2::video::{Window, WindowBuildError};
use sdl2::VideoSubsystem;
use thiserror::Error;
use tracing::{debug, info, info_span};
use tracing_subscriber::{fmt, EnvFilter};

struct SaveOnDrop {
    emulator: Emulator,
    save_path: PathBuf,
    persist_enabled: bool,
}

#[derive(Debug, Parser)]
#[command(name = "latchboy-desktop")]
struct DesktopArgs {
    /// Path to a ROM file.
    rom_path: PathBuf,
    /// Maximum number of frames to present before exiting.
    #[arg(long, value_parser = clap::value_parser!(u64).range(1..))]
    max_frames: Option<u64>,
    /// CPU cycle step used for each emulation loop iteration.
    #[arg(long, value_parser = clap::value_parser!(u32).range(1..), default_value_t = 1_024)]
    cycle_step: u32,
    /// Optional directory to dump each presented frame as PNG while running headless.
    #[arg(long)]
    frame_output_dir: Option<PathBuf>,
    /// Capture one frame image for every N presented frames.
    ///
    /// For example, `--frame-output-every 60` writes frames 60, 120, 180...
    #[arg(
        long,
        value_parser = clap::value_parser!(u64).range(1..),
        requires = "frame_output_dir"
    )]
    frame_output_every: Option<u64>,
    /// Capture only the final presented frame as `frame-last.png`.
    #[arg(
        long,
        requires = "frame_output_dir",
        conflicts_with = "frame_output_every"
    )]
    frame_output_last_only: bool,
}

impl Drop for SaveOnDrop {
    fn drop(&mut self) {
        if self.persist_enabled {
            persist_save_data(self.emulator.cartridge(), &self.save_path);
        }
    }
}

#[derive(Debug, Error)]
enum SurfaceError {
    #[error("surface update failed")]
    InvalidSurfaceLength,
    #[error("failed to write frame image: {0}")]
    FrameImageWrite(String),
}

/// SDL-backed frame presenter.
struct SdlPresenter {
    canvas: Canvas<Window>,
    event_pump: sdl2::EventPump,
    texture: Texture,
    buffer: Vec<u32>,
    rgb_buffer: Vec<u8>,
    presented_frames: u64,
    max_frames: u64,
    close_requested: bool,
    frame_capture: Option<FrameCaptureConfig>,
}

impl SdlPresenter {
    fn build_window(video: &VideoSubsystem) -> Result<Window, WindowBuildError> {
        video
            .window(
                "Latchboy",
                (FRAMEBUFFER_WIDTH as u32) * 3,
                (FRAMEBUFFER_HEIGHT as u32) * 3,
            )
            .position_centered()
            .build()
    }

    fn build_canvas(video: &VideoSubsystem) -> io::Result<Canvas<Window>> {
        let mut last_error: Option<io::Error> = None;

        for attempt in [
            "accelerated + vsync",
            "accelerated",
            "software",
            "default renderer",
        ] {
            let window = Self::build_window(video).map_err(io::Error::other)?;
            let mut builder = window.into_canvas();
            builder = match attempt {
                "accelerated + vsync" => builder.accelerated().present_vsync(),
                "accelerated" => builder.accelerated(),
                "software" => builder.software(),
                _ => builder,
            };

            match builder.build() {
                Ok(canvas) => {
                    info!(renderer_profile = attempt, "initialized SDL renderer");
                    return Ok(canvas);
                }
                Err(error) => {
                    debug!(renderer_profile = attempt, %error, "SDL renderer init failed");
                    last_error = Some(io::Error::other(error));
                }
            }
        }

        Err(last_error.unwrap_or_else(|| io::Error::other("failed to build SDL renderer")))
    }

    fn new(max_frames: u64, frame_capture: Option<FrameCaptureConfig>) -> io::Result<Self> {
        if let Some(capture) = &frame_capture {
            fs::create_dir_all(&capture.output_dir)?;
        }

        let sdl = sdl2::init().map_err(io::Error::other)?;
        let video = sdl.video().map_err(io::Error::other)?;
        let canvas = Self::build_canvas(&video)?;
        let texture_creator = canvas.texture_creator();
        let texture = texture_creator
            .create_texture_streaming(
                PixelFormatEnum::RGB24,
                FRAMEBUFFER_WIDTH as u32,
                FRAMEBUFFER_HEIGHT as u32,
            )
            .map_err(io::Error::other)?;
        let event_pump = sdl.event_pump().map_err(io::Error::other)?;

        Ok(Self {
            canvas,
            event_pump,
            texture,
            buffer: vec![0; FRAMEBUFFER_LEN],
            rgb_buffer: vec![0; FRAMEBUFFER_LEN * 3],
            presented_frames: 0,
            max_frames,
            close_requested: false,
            frame_capture,
        })
    }

    fn flush_final_frame_capture(&self) -> Result<(), SurfaceError> {
        let Some(capture) = &self.frame_capture else {
            return Ok(());
        };

        if !matches!(capture.mode, FrameCaptureMode::LastOnly) || self.presented_frames == 0 {
            return Ok(());
        }

        let path = capture.output_dir.join("frame-last.png");
        write_rgb_surface_to_png(
            &path,
            &self.buffer,
            FRAMEBUFFER_WIDTH as u32,
            FRAMEBUFFER_HEIGHT as u32,
        )
        .map_err(|error| SurfaceError::FrameImageWrite(error.to_string()))
    }
}

#[derive(Clone, Copy, Debug)]
enum FrameCaptureMode {
    Every { interval: u64 },
    LastOnly,
}

#[derive(Clone, Debug)]
struct FrameCaptureConfig {
    output_dir: PathBuf,
    mode: FrameCaptureMode,
}

impl FrameCaptureConfig {
    fn should_capture(&self, frame_index: u64) -> bool {
        match self.mode {
            FrameCaptureMode::Every { interval } => frame_index.is_multiple_of(interval),
            FrameCaptureMode::LastOnly => false,
        }
    }
}

impl FramePresenter for SdlPresenter {
    type Error = SurfaceError;

    fn is_open(&self) -> bool {
        !self.close_requested && self.presented_frames < self.max_frames
    }

    fn poll_events(&mut self) -> Result<(), Self::Error> {
        for event in self.event_pump.poll_iter() {
            match event {
                Event::Quit { .. }
                | Event::KeyDown {
                    keycode: Some(Keycode::Escape),
                    ..
                } => {
                    self.close_requested = true;
                    break;
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn present_frame(&mut self, surface: &[u32]) -> Result<(), Self::Error> {
        if surface.len() != FRAMEBUFFER_LEN {
            return Err(SurfaceError::InvalidSurfaceLength);
        }

        self.buffer.copy_from_slice(surface);

        for (index, &pixel) in surface.iter().enumerate() {
            let base = index * 3;
            self.rgb_buffer[base] = ((pixel >> 16) & 0xFF) as u8;
            self.rgb_buffer[base + 1] = ((pixel >> 8) & 0xFF) as u8;
            self.rgb_buffer[base + 2] = (pixel & 0xFF) as u8;
        }

        self.texture
            .update(None, &self.rgb_buffer, FRAMEBUFFER_WIDTH * 3)
            .map_err(|_| SurfaceError::InvalidSurfaceLength)?;

        self.canvas.clear();
        self.canvas
            .copy(&self.texture, None, None)
            .map_err(|_| SurfaceError::InvalidSurfaceLength)?;
        self.canvas.present();

        let frame_index = self.presented_frames + 1;
        if let Some(capture) = &self.frame_capture {
            let frame_path = match capture.mode {
                FrameCaptureMode::Every { .. } if capture.should_capture(frame_index) => Some(
                    capture
                        .output_dir
                        .join(format!("frame-{frame_index:06}.png")),
                ),
                FrameCaptureMode::LastOnly => None,
                _ => None,
            };

            if let Some(path) = frame_path {
                write_rgb_surface_to_png(
                    &path,
                    &self.buffer,
                    FRAMEBUFFER_WIDTH as u32,
                    FRAMEBUFFER_HEIGHT as u32,
                )
                .map_err(|error| SurfaceError::FrameImageWrite(error.to_string()))?;
            }
        }

        self.presented_frames += 1;
        Ok(())
    }
}

fn iteration_budget_for_frames(frame_budget: u64, cycle_step: u32) -> u64 {
    const DMG_FRAME_CYCLES: u64 = 70_224;
    let step = u64::from(cycle_step.max(1));
    let iterations_per_frame = DMG_FRAME_CYCLES.div_ceil(step);
    frame_budget.saturating_mul(iterations_per_frame.saturating_mul(2))
}

fn frame_budget_from_env() -> u64 {
    env::var("LATCHBOY_DESKTOP_MAX_FRAMES")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|&value| value > 0)
        .unwrap_or(300)
}

fn init_tracing() {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = fmt()
        .with_env_filter(env_filter)
        .compact()
        .with_target(false)
        .try_init();
}

fn main() -> ExitCode {
    init_tracing();
    let args = DesktopArgs::parse();
    let rom_path = args.rom_path;
    info!("desktop runner starting");

    let rom_load_span = info_span!("rom_load", rom_path = %rom_path.display());
    let _rom_load_guard = rom_load_span.enter();
    let rom_data = match fs::read(&rom_path) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!(
                "error: failed to read ROM '{}': {error}",
                rom_path.display()
            );
            return ExitCode::FAILURE;
        }
    };
    info!(rom_size = rom_data.len(), "rom loaded");
    drop(_rom_load_guard);

    let cartridge_span = info_span!("cartridge_parse", rom_path = %rom_path.display());
    let _cartridge_guard = cartridge_span.enter();
    let mut cartridge = match Cartridge::from_rom(rom_data) {
        Ok(cartridge) => cartridge,
        Err(error) => {
            eprintln!(
                "error: failed to parse cartridge from ROM '{}': {error:?}",
                rom_path.display()
            );
            return ExitCode::FAILURE;
        }
    };
    info!("cartridge parsed");
    drop(_cartridge_guard);

    let save_path = save_path_from_rom_path(&rom_path);
    let load_status = load_save_data_if_available(&mut cartridge, &save_path);
    let persist_enabled = should_persist_after_load(load_status);

    let mut runtime = SaveOnDrop {
        emulator: Emulator::from_cartridge(cartridge),
        save_path,
        persist_enabled,
    };
    let frame_budget = args.max_frames.unwrap_or_else(frame_budget_from_env);
    let iteration_budget = iteration_budget_for_frames(frame_budget, args.cycle_step);
    debug!(
        frame_budget,
        iteration_budget,
        cycle_step = args.cycle_step,
        "computed run budgets"
    );
    let frame_capture = args.frame_output_dir.map(|output_dir| FrameCaptureConfig {
        output_dir,
        mode: if args.frame_output_last_only {
            FrameCaptureMode::LastOnly
        } else {
            FrameCaptureMode::Every {
                interval: args.frame_output_every.unwrap_or(1),
            }
        },
    });

    let mut surface = match SdlPresenter::new(frame_budget, frame_capture) {
        Ok(surface) => surface,
        Err(error) => {
            eprintln!("error: failed to initialize output surface: {error}");
            return ExitCode::FAILURE;
        }
    };

    let frame_loop_span = info_span!("frame_loop");
    let _frame_loop_guard = frame_loop_span.enter();
    info!("frame loop starting");
    let frames_presented = match run_emulation_loop(
        &mut runtime.emulator,
        &mut surface,
        args.cycle_step,
        Some(frame_budget),
        Some(iteration_budget),
    ) {
        Ok(frames) => frames,
        Err(error) => {
            eprintln!("error: emulation loop aborted: {error}");
            return ExitCode::FAILURE;
        }
    };
    info!(frames_presented, "frame loop ended");
    if let Err(error) = surface.flush_final_frame_capture() {
        eprintln!("error: failed to flush final frame capture: {error}");
        return ExitCode::FAILURE;
    }
    if frames_presented >= frame_budget {
        debug!(frames_presented, frame_budget, "frame budget exhaustion");
    }
    debug!(iteration_budget, "iteration budget configured");

    println!(
        "Latchboy desktop frame loop completed: rendered {} frames into {}x{} surface",
        frames_presented, FRAMEBUFFER_WIDTH, FRAMEBUFFER_HEIGHT
    );
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::{FrameCaptureConfig, FrameCaptureMode};
    use std::path::PathBuf;

    #[test]
    fn capture_every_interval_selects_expected_frames() {
        let config = FrameCaptureConfig {
            output_dir: PathBuf::from("unused"),
            mode: FrameCaptureMode::Every { interval: 3 },
        };
        assert!(!config.should_capture(1));
        assert!(!config.should_capture(2));
        assert!(config.should_capture(3));
        assert!(config.should_capture(6));
    }

    #[test]
    fn last_only_mode_defers_capture_until_shutdown() {
        let config = FrameCaptureConfig {
            output_dir: PathBuf::from("unused"),
            mode: FrameCaptureMode::LastOnly,
        };
        assert!(!config.should_capture(1));
        assert!(!config.should_capture(99));
    }
}
