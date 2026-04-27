use std::env;
use std::fs;
use std::io::{self, BufRead};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::mpsc::{self, Receiver};
use std::thread;

use clap::Parser;
use latchboy_core::{
    cartridge::Cartridge, Emulator, FRAMEBUFFER_HEIGHT, FRAMEBUFFER_LEN, FRAMEBUFFER_WIDTH,
};
use latchboy_desktop::savefile::{
    load_save_data_if_available, persist_save_data, save_path_from_rom_path,
    should_persist_after_load,
};
use latchboy_desktop::{run_emulation_loop, write_rgb_surface_to_png, FramePresenter};
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

/// Minimal headless-friendly window surface buffer.
struct WindowSurface {
    buffer: Vec<u32>,
    presented_frames: u64,
    max_frames: u64,
    close_requested: bool,
    input_events: Receiver<String>,
    frame_output_dir: Option<PathBuf>,
}

impl WindowSurface {
    fn new(max_frames: u64, frame_output_dir: Option<PathBuf>) -> io::Result<Self> {
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let stdin = io::stdin();
            for line in stdin.lock().lines() {
                match line {
                    Ok(event) => {
                        if tx.send(event).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        if let Some(dir) = &frame_output_dir {
            fs::create_dir_all(dir)?;
        }

        Ok(Self {
            buffer: vec![0; FRAMEBUFFER_LEN],
            presented_frames: 0,
            max_frames,
            close_requested: false,
            input_events: rx,
            frame_output_dir,
        })
    }
}

impl FramePresenter for WindowSurface {
    type Error = SurfaceError;

    fn is_open(&self) -> bool {
        !self.close_requested && self.presented_frames < self.max_frames
    }

    fn poll_events(&mut self) -> Result<(), Self::Error> {
        while let Ok(event) = self.input_events.try_recv() {
            if event.trim().eq_ignore_ascii_case("q") || event.trim().eq_ignore_ascii_case("quit") {
                self.close_requested = true;
                return Ok(());
            }
        }
        Ok(())
    }

    fn present_frame(&mut self, surface: &[u32]) -> Result<(), Self::Error> {
        if surface.len() != FRAMEBUFFER_LEN {
            return Err(SurfaceError::InvalidSurfaceLength);
        }
        self.buffer.copy_from_slice(surface);
        let frame_index = self.presented_frames + 1;
        if let Some(dir) = &self.frame_output_dir {
            let frame_path = dir.join(format!("frame-{frame_index:06}.png"));
            write_rgb_surface_to_png(
                &frame_path,
                &self.buffer,
                FRAMEBUFFER_WIDTH as u32,
                FRAMEBUFFER_HEIGHT as u32,
            )
            .map_err(|error| SurfaceError::FrameImageWrite(error.to_string()))?;
        }
        self.presented_frames += 1;
        if self.presented_frames.is_multiple_of(60) {
            println!(
                "presented {} frames at {}x{} (type 'q' then Enter to quit)",
                self.presented_frames, FRAMEBUFFER_WIDTH, FRAMEBUFFER_HEIGHT
            );
        }
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
    let mut surface = match WindowSurface::new(frame_budget, args.frame_output_dir) {
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
