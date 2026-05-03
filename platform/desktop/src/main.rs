use std::collections::HashMap;
use std::env;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::process::ExitCode;
use std::thread;
use std::time::{Duration, Instant};

use clap::{ArgAction, Parser};
use latchboy_core::{
    cartridge::Cartridge, Emulator, JoypadButton, FRAMEBUFFER_HEIGHT, FRAMEBUFFER_LEN,
    FRAMEBUFFER_WIDTH,
};
use latchboy_desktop::savefile::{
    load_save_data_if_available, persist_save_data, save_path_from_rom_path,
    should_persist_after_load,
};
use latchboy_desktop::{run_emulation_loop, write_rgb_surface_to_png, FramePresenter};
use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::pixels::{Color, PixelFormatEnum};
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
    /// Enable 60Hz presentation pacing (vsync-style frame limiting).
    #[arg(long = "vsync", action = ArgAction::SetTrue, default_value_t = true, overrides_with = "no_vsync")]
    vsync: bool,
    /// Disable 60Hz presentation pacing.
    #[arg(long = "no-vsync", action = ArgAction::SetTrue, overrides_with = "vsync")]
    no_vsync: bool,
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
    #[arg(long, default_value = "x")]
    key_a: String,
    #[arg(long, default_value = "z")]
    key_b: String,
    #[arg(long, default_value = "backspace")]
    key_select: String,
    #[arg(long, default_value = "return")]
    key_start: String,
    #[arg(long, default_value = "right")]
    key_right: String,
    #[arg(long, default_value = "left")]
    key_left: String,
    #[arg(long, default_value = "up")]
    key_up: String,
    #[arg(long, default_value = "down")]
    key_down: String,
}

impl DesktopArgs {
    fn vsync_enabled(&self) -> bool {
        !self.no_vsync && self.vsync
    }
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
    #[error("invalid surface length")]
    InvalidSurfaceLength,
    #[error("SDL texture update failed: {0}")]
    TextureUpdate(String),
    #[error("SDL canvas copy failed: {0}")]
    CanvasCopy(String),
    #[error("failed to write frame image: {0}")]
    FrameImageWrite(String),
}

/// SDL-backed frame presenter.
struct SdlPresenter {
    window: Window,
    event_pump: sdl2::EventPump,
    buffer: Vec<u32>,
    presented_frames: u64,
    max_frames: u64,
    close_requested: bool,
    frame_capture: Option<FrameCaptureConfig>,
    target_frame_duration: Option<Duration>,
    next_frame_deadline: Option<Instant>,
    keymap: Vec<(Keycode, JoypadButton)>,
    pending_input_events: Vec<(JoypadButton, bool)>,
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

    fn new(
        max_frames: u64,
        frame_capture: Option<FrameCaptureConfig>,
        vsync: bool,
        keymap: Vec<(Keycode, JoypadButton)>,
    ) -> io::Result<Self> {
        if let Some(capture) = &frame_capture {
            fs::create_dir_all(&capture.output_dir)?;
        }

        let sdl = sdl2::init().map_err(io::Error::other)?;
        let video = sdl.video().map_err(io::Error::other)?;
        let window = Self::build_window(&video).map_err(io::Error::other)?;
        info!("initialized SDL window surface presenter");
        let event_pump = sdl.event_pump().map_err(io::Error::other)?;

        Ok(Self {
            window,
            event_pump,
            buffer: vec![0; FRAMEBUFFER_LEN],
            presented_frames: 0,
            max_frames,
            close_requested: false,
            frame_capture,
            target_frame_duration: vsync.then(|| Duration::from_secs_f64(1.0 / 60.0)),
            next_frame_deadline: None,
            keymap,
            pending_input_events: Vec::new(),
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
        let keymap = self.keymap.clone();
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
                Event::KeyDown {
                    keycode: Some(key),
                    repeat: false,
                    ..
                } => {
                    if let Some(button) = keymap
                        .iter()
                        .find_map(|(mapped_key, button)| (*mapped_key == key).then_some(*button))
                    {
                        self.pending_input_events.push((button, true));
                    }
                }
                Event::KeyUp {
                    keycode: Some(key),
                    repeat: false,
                    ..
                } => {
                    if let Some(button) = keymap
                        .iter()
                        .find_map(|(mapped_key, button)| (*mapped_key == key).then_some(*button))
                    {
                        self.pending_input_events.push((button, false));
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn drain_input_events(&mut self) -> Vec<(JoypadButton, bool)> {
        std::mem::take(&mut self.pending_input_events)
    }

    fn present_frame(&mut self, surface: &[u32]) -> Result<(), Self::Error> {
        if surface.len() != FRAMEBUFFER_LEN {
            return Err(SurfaceError::InvalidSurfaceLength);
        }

        self.buffer.copy_from_slice(surface);
        let now = Instant::now();
        if let (Some(deadline), Some(_)) = (self.next_frame_deadline, self.target_frame_duration) {
            if deadline > now {
                thread::sleep(deadline - now);
            }
        }

        let mut window_surface = self
            .window
            .surface(&self.event_pump)
            .map_err(|error| SurfaceError::TextureUpdate(error.to_string()))?;
        let surface_width = window_surface.width() as usize;
        let surface_height = window_surface.height() as usize;
        let pitch = window_surface.pitch() as usize;
        let pixel_format_enum = window_surface.pixel_format_enum();
        let bytes_per_pixel = pixel_format_enum.byte_size_per_pixel();
        let pixel_format = window_surface.pixel_format();
        let scale_x = (surface_width / FRAMEBUFFER_WIDTH).max(1);
        let scale_y = (surface_height / FRAMEBUFFER_HEIGHT).max(1);

        window_surface.with_lock_mut(|pixels| {
            for y in 0..FRAMEBUFFER_HEIGHT {
                for x in 0..FRAMEBUFFER_WIDTH {
                    let pixel = surface[y * FRAMEBUFFER_WIDTH + x];
                    let r = ((pixel >> 16) & 0xFF) as u8;
                    let g = ((pixel >> 8) & 0xFF) as u8;
                    let b = (pixel & 0xFF) as u8;
                    for sy in 0..scale_y {
                        let dy = y * scale_y + sy;
                        if dy >= surface_height {
                            continue;
                        }
                        let row = dy * pitch;
                        for sx in 0..scale_x {
                            let dx = x * scale_x + sx;
                            if dx >= surface_width {
                                continue;
                            }
                            let offset = row + (dx * bytes_per_pixel);
                            match pixel_format_enum {
                                PixelFormatEnum::RGB24 => {
                                    pixels[offset] = r;
                                    pixels[offset + 1] = g;
                                    pixels[offset + 2] = b;
                                }
                                PixelFormatEnum::BGR24 => {
                                    pixels[offset] = b;
                                    pixels[offset + 1] = g;
                                    pixels[offset + 2] = r;
                                }
                                _ => {
                                    let mapped =
                                        Color::RGB(r, g, b).to_u32(&pixel_format).to_ne_bytes();
                                    pixels[offset..offset + bytes_per_pixel]
                                        .copy_from_slice(&mapped[..bytes_per_pixel]);
                                }
                            }
                        }
                    }
                }
            }
        });
        window_surface
            .update_window()
            .map_err(|error| SurfaceError::CanvasCopy(error.to_string()))?;

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
        if let Some(target_frame_duration) = self.target_frame_duration {
            self.next_frame_deadline =
                Some(self.next_frame_deadline.unwrap_or_else(Instant::now) + target_frame_duration);
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

fn build_keymap(args: &DesktopArgs) -> Result<Vec<(Keycode, JoypadButton)>, String> {
    let mappings = [
        (&args.key_a, JoypadButton::A),
        (&args.key_b, JoypadButton::B),
        (&args.key_select, JoypadButton::Select),
        (&args.key_start, JoypadButton::Start),
        (&args.key_right, JoypadButton::Right),
        (&args.key_left, JoypadButton::Left),
        (&args.key_up, JoypadButton::Up),
        (&args.key_down, JoypadButton::Down),
    ];
    let mut keymap = Vec::with_capacity(mappings.len());
    let mut assigned_keys: HashMap<Keycode, JoypadButton> = HashMap::new();
    for (key_name, button) in mappings {
        let key = Keycode::from_name(key_name)
            .ok_or_else(|| format!("unknown key '{key_name}' for {:?}", button))?;
        if key == Keycode::Escape {
            return Err(format!(
                "key '{key_name}' is reserved for quit and cannot be mapped to {:?}",
                button
            ));
        }
        if let Some(existing_button) = assigned_keys.insert(key, button) {
            return Err(format!(
                "duplicate key binding '{key_name}' for {:?} and {:?}",
                existing_button, button
            ));
        }
        keymap.push((key, button));
    }
    Ok(keymap)
}

fn main() -> ExitCode {
    init_tracing();
    let args = DesktopArgs::parse();
    let keymap = match build_keymap(&args) {
        Ok(keymap) => keymap,
        Err(error) => {
            eprintln!("error: invalid key mapping: {error}");
            return ExitCode::FAILURE;
        }
    };
    let vsync_enabled = args.vsync_enabled();
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

    let mut surface = match SdlPresenter::new(frame_budget, frame_capture, vsync_enabled, keymap) {
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
    use super::{build_keymap, DesktopArgs, FrameCaptureConfig, FrameCaptureMode};
    use clap::Parser;
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

    #[test]
    fn cli_accepts_explicit_no_vsync_flag() {
        let args = DesktopArgs::try_parse_from(["latchboy-desktop", "game.gb", "--no-vsync"])
            .expect("--no-vsync should be accepted");
        assert!(args.no_vsync);
        assert!(!args.vsync_enabled());
    }

    #[test]
    fn cli_defaults_to_vsync_enabled() {
        let args = DesktopArgs::try_parse_from(["latchboy-desktop", "game.gb"])
            .expect("default args should parse");
        assert!(args.vsync);
        assert!(!args.no_vsync);
        assert!(args.vsync_enabled());
    }

    #[test]
    fn duplicate_key_mappings_are_rejected() {
        let args = DesktopArgs::try_parse_from([
            "latchboy-desktop",
            "game.gb",
            "--key-a",
            "x",
            "--key-b",
            "x",
        ])
        .expect("args should parse");
        let error = build_keymap(&args).expect_err("duplicate key binding should be rejected");
        assert!(error.contains("duplicate key binding"));
    }

    #[test]
    fn escape_key_mapping_is_rejected() {
        let args =
            DesktopArgs::try_parse_from(["latchboy-desktop", "game.gb", "--key-start", "escape"])
                .expect("args should parse");
        let error = build_keymap(&args).expect_err("escape should be rejected as a mapping");
        assert!(error.contains("reserved for quit"));
    }
}
