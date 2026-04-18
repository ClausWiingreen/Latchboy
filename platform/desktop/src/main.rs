use std::env;
use std::error::Error;
use std::fmt;
use std::fs;
use std::io::{self, BufRead};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::mpsc::{self, Receiver};
use std::thread;

use latchboy_core::{
    cartridge::Cartridge, Emulator, FRAMEBUFFER_HEIGHT, FRAMEBUFFER_LEN, FRAMEBUFFER_WIDTH,
};
use latchboy_desktop::savefile::{
    load_save_data_if_available, persist_save_data, save_path_from_rom_path,
    should_persist_after_load,
};
use latchboy_desktop::{run_emulation_loop, FramePresenter};

struct SaveOnDrop {
    emulator: Emulator,
    save_path: PathBuf,
    persist_enabled: bool,
}

impl Drop for SaveOnDrop {
    fn drop(&mut self) {
        if self.persist_enabled {
            persist_save_data(self.emulator.cartridge(), &self.save_path);
        }
    }
}

#[derive(Debug)]
struct SurfaceError;

impl fmt::Display for SurfaceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "surface update failed")
    }
}

impl Error for SurfaceError {}

/// Minimal headless-friendly window surface buffer.
struct WindowSurface {
    buffer: Vec<u32>,
    presented_frames: u64,
    max_frames: u64,
    close_requested: bool,
    input_events: Receiver<String>,
}

impl WindowSurface {
    fn new(max_frames: u64) -> Self {
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

        Self {
            buffer: vec![0; FRAMEBUFFER_LEN],
            presented_frames: 0,
            max_frames,
            close_requested: false,
            input_events: rx,
        }
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
            }
        }
        Ok(())
    }

    fn present_frame(&mut self, surface: &[u32]) -> Result<(), Self::Error> {
        if surface.len() != FRAMEBUFFER_LEN {
            return Err(SurfaceError);
        }
        self.buffer.copy_from_slice(surface);
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

fn main() -> ExitCode {
    let rom_path = match env::args().nth(1) {
        Some(path) => PathBuf::from(path),
        None => {
            eprintln!("usage: latchboy-desktop <path-to-rom.gb>");
            return ExitCode::FAILURE;
        }
    };

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

    let save_path = save_path_from_rom_path(&rom_path);
    let load_status = load_save_data_if_available(&mut cartridge, &save_path);
    let persist_enabled = should_persist_after_load(load_status);

    let mut runtime = SaveOnDrop {
        emulator: Emulator::from_cartridge(cartridge),
        save_path,
        persist_enabled,
    };
    let frame_budget = frame_budget_from_env();
    let iteration_budget = iteration_budget_for_frames(frame_budget, 1_024);
    let mut surface = WindowSurface::new(frame_budget);

    let frames_presented = match run_emulation_loop(
        &mut runtime.emulator,
        &mut surface,
        1_024,
        Some(frame_budget),
        Some(iteration_budget),
    ) {
        Ok(frames) => frames,
        Err(error) => {
            eprintln!("error: emulation loop aborted: {error}");
            return ExitCode::FAILURE;
        }
    };

    println!(
        "Latchboy desktop frame loop completed: rendered {} frames into {}x{} surface",
        frames_presented, FRAMEBUFFER_WIDTH, FRAMEBUFFER_HEIGHT
    );
    ExitCode::SUCCESS
}
