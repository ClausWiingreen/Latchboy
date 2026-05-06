pub mod debug_harness;
pub mod savefile;

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;
use std::thread;
use std::time::Duration;

use latchboy_core::{Emulator, JoypadButton, FRAMEBUFFER_LEN};
use png::{BitDepth, ColorType, Encoder};
use thiserror::Error;

const DMG_FRAME_CYCLES: u32 = 70_224;
// `Emulator::step_cycles` advances by at least the requested cycles and can overshoot by
// up to one CPU instruction. The longest currently implemented instruction is 24 cycles,
// so leave that much headroom to avoid skipping past multiple frame-ready pulses in one step.
const MAX_CPU_INSTRUCTION_CYCLES: u32 = 24;
const MAX_CYCLES_BETWEEN_FRAME_POLLS: u32 = DMG_FRAME_CYCLES - MAX_CPU_INSTRUCTION_CYCLES;
const PAUSED_POLL_SLEEP: Duration = Duration::from_millis(1);
const AUDIO_PULL_SAMPLES: usize = 1_024;

const JOYPAD_BUTTONS: [JoypadButton; 8] = [
    JoypadButton::A,
    JoypadButton::B,
    JoypadButton::Select,
    JoypadButton::Start,
    JoypadButton::Right,
    JoypadButton::Left,
    JoypadButton::Up,
    JoypadButton::Down,
];

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

pub type DesktopResult<T> = Result<T, DesktopRuntimeError>;
pub type AudioCallback<'a> = &'a mut dyn FnMut(&[i16]) -> DesktopResult<()>;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DesktopRuntimeError {
    #[error("cycle_step must be greater than zero")]
    InvalidCycleStep,
    #[error("{0}")]
    FrameBlit(#[from] FrameBlitError),
    #[error("frame presentation failed: {0}")]
    FramePresentation(String),
    #[error("audio output failed: {0}")]
    AudioOutput(String),
}

impl DesktopRuntimeError {
    pub fn frame_presentation(error: impl std::fmt::Display) -> Self {
        Self::FramePresentation(error.to_string())
    }

    pub fn audio_output(error: impl std::fmt::Display) -> Self {
        Self::AudioOutput(error.to_string())
    }
}

pub trait FramePresenter {
    fn is_open(&self) -> bool;
    fn poll_events(&mut self) -> DesktopResult<()>;
    fn drain_input_events(&mut self) -> Vec<(JoypadButton, bool)> {
        Vec::new()
    }
    fn drain_runtime_events(&mut self) -> Vec<RuntimeEvent> {
        Vec::new()
    }
    fn present_frame(&mut self, surface: &[u32]) -> DesktopResult<()>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeEvent {
    Reset,
    ReloadRom,
    SaveState { slot: u8 },
    LoadState { slot: u8 },
    SetPaused(bool),
    StepFrame,
    SetFastForward(bool),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EmulationRunStats {
    pub frames_presented: u64,
    pub iterations: u64,
    pub resets_triggered: u64,
    pub reloads_triggered: u64,
}

impl EmulationRunStats {
    fn record_runtime_outcome(&mut self, outcome: RuntimeControlOutcome) {
        self.resets_triggered = self
            .resets_triggered
            .saturating_add(outcome.resets_triggered);
        self.reloads_triggered = self
            .reloads_triggered
            .saturating_add(outcome.reloads_triggered);
    }
}

#[derive(Debug, Default)]
pub struct RuntimeSessionState {
    pub save_state_slots: HashMap<u8, Emulator>,
    pub pressed_buttons: HashSet<JoypadButton>,
    pub paused: bool,
    pub frame_steps_remaining: u64,
    pub fast_forward: bool,
    pub reload_requested: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RuntimeControlOutcome {
    pub resets_triggered: u64,
    pub reloads_triggered: u64,
    pub presenter_open: bool,
    pub reload_requested: bool,
}

impl RuntimeControlOutcome {
    pub fn should_stop(&self) -> bool {
        !self.presenter_open || self.reload_requested
    }
}

#[derive(Debug, Default)]
pub struct RuntimeController;

impl RuntimeController {
    pub fn poll_and_apply<P: FramePresenter>(
        presenter: &mut P,
        emulator: &mut Emulator,
        runtime_state: &mut RuntimeSessionState,
    ) -> DesktopResult<RuntimeControlOutcome> {
        presenter.poll_events()?;

        let mut outcome = RuntimeControlOutcome::default();
        for event in presenter.drain_runtime_events() {
            match event {
                RuntimeEvent::Reset => {
                    emulator.reset();
                    outcome.resets_triggered = outcome.resets_triggered.saturating_add(1);
                }
                RuntimeEvent::ReloadRom => {
                    runtime_state.reload_requested = true;
                    outcome.reloads_triggered = outcome.reloads_triggered.saturating_add(1);
                }
                RuntimeEvent::SaveState { slot } => {
                    runtime_state
                        .save_state_slots
                        .insert(slot, emulator.clone());
                }
                RuntimeEvent::LoadState { slot } => {
                    if let Some(saved) = runtime_state.save_state_slots.get(&slot) {
                        *emulator = saved.clone();
                        for button in JOYPAD_BUTTONS {
                            emulator.set_button_pressed(
                                button,
                                runtime_state.pressed_buttons.contains(&button),
                            );
                        }
                    }
                }
                RuntimeEvent::SetPaused(paused) => {
                    runtime_state.paused = paused;
                    if !paused {
                        runtime_state.frame_steps_remaining = 0;
                    }
                }
                RuntimeEvent::StepFrame => {
                    if runtime_state.paused {
                        runtime_state.frame_steps_remaining =
                            runtime_state.frame_steps_remaining.saturating_add(1);
                    }
                }
                RuntimeEvent::SetFastForward(enabled) => runtime_state.fast_forward = enabled,
            }
        }

        for (button, pressed) in presenter.drain_input_events() {
            if pressed {
                runtime_state.pressed_buttons.insert(button);
            } else {
                runtime_state.pressed_buttons.remove(&button);
            }
            emulator.set_button_pressed(button, pressed);
        }

        outcome.presenter_open = presenter.is_open();
        outcome.reload_requested = runtime_state.reload_requested;
        Ok(outcome)
    }
}

/// Runs emulation loop with caller-owned runtime state slots that can persist across invocations.
pub fn run_emulation_loop<P: FramePresenter>(
    emulator: &mut Emulator,
    presenter: &mut P,
    cycle_step: u32,
    frame_limit: Option<u64>,
    iteration_limit: Option<u64>,
    runtime_state: &mut RuntimeSessionState,
    mut audio_sink: Option<AudioCallback<'_>>,
) -> DesktopResult<EmulationRunStats> {
    if cycle_step == 0 {
        return Err(DesktopRuntimeError::InvalidCycleStep);
    }

    fn present_if_ready<P: FramePresenter>(
        emulator: &mut Emulator,
        presenter: &mut P,
        surface: &mut [u32],
    ) -> DesktopResult<bool> {
        if !emulator.take_frame_ready() {
            return Ok(false);
        }

        blit_dmg_framebuffer_to_rgb_surface(emulator.framebuffer_pixels(), surface)?;
        presenter.present_frame(surface)?;
        Ok(true)
    }

    let mut surface = vec![0u32; FRAMEBUFFER_LEN];
    let mut stats = EmulationRunStats::default();

    while presenter.is_open() {
        if let Some(limit) = frame_limit {
            if stats.frames_presented >= limit {
                break;
            }
        }
        let outcome = RuntimeController::poll_and_apply(presenter, emulator, runtime_state)?;
        let should_stop = outcome.should_stop();
        stats.record_runtime_outcome(outcome);
        if should_stop {
            break;
        }
        if runtime_state.paused && runtime_state.frame_steps_remaining == 0 {
            thread::sleep(PAUSED_POLL_SLEEP);
            continue;
        }
        if present_if_ready(emulator, presenter, &mut surface)? {
            stats.frames_presented += 1;
            if runtime_state.paused && runtime_state.frame_steps_remaining > 0 {
                runtime_state.frame_steps_remaining -= 1;
            }
            continue;
        }
        let mut cycles_remaining = cycle_step;
        while cycles_remaining != 0 && presenter.is_open() {
            if let Some(limit) = iteration_limit {
                if stats.iterations >= limit {
                    return Ok(stats);
                }
            }
            if let Some(limit) = frame_limit {
                if stats.frames_presented >= limit {
                    return Ok(stats);
                }
            }
            let outcome = RuntimeController::poll_and_apply(presenter, emulator, runtime_state)?;
            let should_stop = outcome.should_stop();
            stats.record_runtime_outcome(outcome);
            if should_stop {
                return Ok(stats);
            }
            if runtime_state.paused && runtime_state.frame_steps_remaining == 0 {
                thread::sleep(PAUSED_POLL_SLEEP);
                continue;
            }

            if present_if_ready(emulator, presenter, &mut surface)? {
                stats.frames_presented += 1;
                if runtime_state.paused && runtime_state.frame_steps_remaining > 0 {
                    runtime_state.frame_steps_remaining -= 1;
                }
                continue;
            }

            let chunk = cycles_remaining.min(MAX_CYCLES_BETWEEN_FRAME_POLLS);
            emulator.step_cycles(chunk);
            if let Some(sink) = audio_sink.as_deref_mut() {
                let queued_samples = emulator.queued_audio_samples();
                if queued_samples != 0 {
                    let request = queued_samples.min(AUDIO_PULL_SAMPLES);
                    let samples = emulator.pull_audio_samples(request);
                    sink(&samples)?;
                }
            }
            cycles_remaining -= chunk;
            stats.iterations += 1;
        }
    }

    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::{
        run_emulation_loop, DesktopResult, FramePresenter, RuntimeController, RuntimeEvent,
        RuntimeSessionState,
    };
    use latchboy_core::{Emulator, JoypadButton};

    struct RuntimeEventOnlyPresenter {
        open: bool,
        events: Vec<RuntimeEvent>,
        input_events: Vec<(latchboy_core::JoypadButton, bool)>,
    }

    impl FramePresenter for RuntimeEventOnlyPresenter {
        fn is_open(&self) -> bool {
            self.open
        }

        fn poll_events(&mut self) -> DesktopResult<()> {
            self.open = false;
            Ok(())
        }

        fn drain_runtime_events(&mut self) -> Vec<RuntimeEvent> {
            std::mem::take(&mut self.events)
        }

        fn drain_input_events(&mut self) -> Vec<(latchboy_core::JoypadButton, bool)> {
            std::mem::take(&mut self.input_events)
        }

        fn present_frame(&mut self, _surface: &[u32]) -> DesktopResult<()> {
            Ok(())
        }
    }

    #[test]
    fn runtime_controller_applies_events_and_reports_outcome_counters() {
        let mut emulator = Emulator::new();
        let mut runtime_state = RuntimeSessionState::default();
        let mut presenter = RuntimeEventOnlyPresenter {
            open: true,
            events: vec![
                RuntimeEvent::Reset,
                RuntimeEvent::SetPaused(true),
                RuntimeEvent::StepFrame,
                RuntimeEvent::SetFastForward(true),
                RuntimeEvent::ReloadRom,
            ],
            input_events: Vec::new(),
        };

        let outcome =
            RuntimeController::poll_and_apply(&mut presenter, &mut emulator, &mut runtime_state)
                .expect("runtime controller should apply events");

        assert_eq!(outcome.resets_triggered, 1);
        assert_eq!(outcome.reloads_triggered, 1);
        assert!(outcome.reload_requested);
        assert!(outcome.should_stop());
        assert!(runtime_state.paused);
        assert_eq!(runtime_state.frame_steps_remaining, 1);
        assert!(runtime_state.fast_forward);
        assert!(runtime_state.reload_requested);
    }

    #[test]
    fn runtime_controller_drains_input_and_synchronizes_pressed_buttons() {
        let mut emulator = Emulator::new();
        let mut runtime_state = RuntimeSessionState::default();
        runtime_state.pressed_buttons.insert(JoypadButton::A);
        let mut presenter = RuntimeEventOnlyPresenter {
            open: true,
            events: Vec::new(),
            input_events: vec![(JoypadButton::B, true), (JoypadButton::A, false)],
        };

        RuntimeController::poll_and_apply(&mut presenter, &mut emulator, &mut runtime_state)
            .expect("runtime controller should drain input events");

        assert!(runtime_state.pressed_buttons.contains(&JoypadButton::B));
        assert!(!runtime_state.pressed_buttons.contains(&JoypadButton::A));
        assert!(presenter.input_events.is_empty());
    }

    #[test]
    fn runtime_controller_save_and_load_slots_restore_emulator_state() {
        let mut emulator = Emulator::new();
        let mut runtime_state = RuntimeSessionState::default();

        emulator.step_cycles(1_024);
        let expected = emulator.clone();
        let mut save_presenter = RuntimeEventOnlyPresenter {
            open: true,
            events: vec![RuntimeEvent::SaveState { slot: 1 }],
            input_events: Vec::new(),
        };
        RuntimeController::poll_and_apply(&mut save_presenter, &mut emulator, &mut runtime_state)
            .expect("runtime controller should save state");

        emulator.reset();

        let mut load_presenter = RuntimeEventOnlyPresenter {
            open: true,
            events: vec![RuntimeEvent::LoadState { slot: 1 }],
            input_events: Vec::new(),
        };
        RuntimeController::poll_and_apply(&mut load_presenter, &mut emulator, &mut runtime_state)
            .expect("runtime controller should load state");

        assert_eq!(emulator, expected);
    }

    #[test]
    fn runtime_state_slots_persist_across_loop_invocations() {
        let mut emulator = Emulator::new();
        let mut runtime_state = RuntimeSessionState::default();

        emulator.step_cycles(1_024);
        let expected = emulator.clone();
        let mut save_presenter = RuntimeEventOnlyPresenter {
            open: true,
            events: vec![RuntimeEvent::SaveState { slot: 1 }],
            input_events: Vec::new(),
        };
        run_emulation_loop(
            &mut emulator,
            &mut save_presenter,
            1,
            None,
            Some(1),
            &mut runtime_state,
            None,
        )
        .expect("save event should be handled");

        emulator.reset();

        let mut load_presenter = RuntimeEventOnlyPresenter {
            open: true,
            events: vec![RuntimeEvent::LoadState { slot: 1 }],
            input_events: Vec::new(),
        };
        run_emulation_loop(
            &mut emulator,
            &mut load_presenter,
            1,
            None,
            Some(1),
            &mut runtime_state,
            None,
        )
        .expect("load event should be handled");

        assert_eq!(emulator, expected);
    }

    #[test]
    fn pause_and_frame_step_events_update_runtime_state() {
        let mut emulator = Emulator::new();
        let mut runtime_state = RuntimeSessionState::default();
        let mut presenter = RuntimeEventOnlyPresenter {
            open: true,
            events: vec![RuntimeEvent::SetPaused(true), RuntimeEvent::StepFrame],
            input_events: Vec::new(),
        };

        run_emulation_loop(
            &mut emulator,
            &mut presenter,
            1,
            None,
            Some(1),
            &mut runtime_state,
            None,
        )
        .expect("pause and frame-step events should be handled");

        assert!(runtime_state.paused);
        assert_eq!(runtime_state.frame_steps_remaining, 1);
    }

    #[test]
    fn frame_step_requires_paused_mode_and_is_cleared_on_resume() {
        let mut emulator = Emulator::new();
        let mut runtime_state = RuntimeSessionState::default();

        let mut running_presenter = RuntimeEventOnlyPresenter {
            open: true,
            events: vec![RuntimeEvent::StepFrame],
            input_events: Vec::new(),
        };
        run_emulation_loop(
            &mut emulator,
            &mut running_presenter,
            1,
            None,
            Some(1),
            &mut runtime_state,
            None,
        )
        .expect("step-frame while running should be ignored");
        assert_eq!(runtime_state.frame_steps_remaining, 0);

        let mut paused_presenter = RuntimeEventOnlyPresenter {
            open: true,
            events: vec![RuntimeEvent::SetPaused(true), RuntimeEvent::StepFrame],
            input_events: Vec::new(),
        };
        run_emulation_loop(
            &mut emulator,
            &mut paused_presenter,
            1,
            None,
            Some(1),
            &mut runtime_state,
            None,
        )
        .expect("step-frame while paused should be queued");
        assert_eq!(runtime_state.frame_steps_remaining, 1);

        let mut resume_presenter = RuntimeEventOnlyPresenter {
            open: true,
            events: vec![RuntimeEvent::SetPaused(false)],
            input_events: Vec::new(),
        };
        run_emulation_loop(
            &mut emulator,
            &mut resume_presenter,
            1,
            None,
            Some(1),
            &mut runtime_state,
            None,
        )
        .expect("resume should clear queued frame steps");
        assert!(!runtime_state.paused);
        assert_eq!(runtime_state.frame_steps_remaining, 0);
    }
}
