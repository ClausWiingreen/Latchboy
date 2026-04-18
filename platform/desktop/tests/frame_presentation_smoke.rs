use std::convert::Infallible;
use std::error::Error;
use std::fmt;

use latchboy_core::Emulator;
use latchboy_desktop::{
    blit_dmg_framebuffer_to_rgb_surface, run_emulation_loop, FramePresenter, DMG_PALETTE_RGB,
};

struct HeadlessPresenter {
    remaining_frames: u64,
    last_frame: Vec<u32>,
    event_polls: u64,
}

struct CloseOnPollPresenter {
    open: bool,
    presents: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PollFailed;

impl fmt::Display for PollFailed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "poll failed")
    }
}

impl Error for PollFailed {}

struct PollErrorAfterSinglePresent {
    open: bool,
    presented_once: bool,
}

impl PollErrorAfterSinglePresent {
    fn new() -> Self {
        Self {
            open: true,
            presented_once: false,
        }
    }
}

impl FramePresenter for PollErrorAfterSinglePresent {
    type Error = PollFailed;

    fn is_open(&self) -> bool {
        self.open
    }

    fn poll_events(&mut self) -> Result<(), Self::Error> {
        if self.presented_once {
            return Err(PollFailed);
        }
        Ok(())
    }

    fn present_frame(&mut self, _surface: &[u32]) -> Result<(), Self::Error> {
        self.presented_once = true;
        Ok(())
    }
}

impl CloseOnPollPresenter {
    fn new() -> Self {
        Self {
            open: true,
            presents: 0,
        }
    }
}

impl FramePresenter for CloseOnPollPresenter {
    type Error = Infallible;

    fn is_open(&self) -> bool {
        self.open
    }

    fn poll_events(&mut self) -> Result<(), Self::Error> {
        self.open = false;
        Ok(())
    }

    fn present_frame(&mut self, _surface: &[u32]) -> Result<(), Self::Error> {
        self.presents += 1;
        Ok(())
    }
}

impl HeadlessPresenter {
    fn new(frame_budget: u64) -> Self {
        Self {
            remaining_frames: frame_budget,
            last_frame: Vec::new(),
            event_polls: 0,
        }
    }
}

impl FramePresenter for HeadlessPresenter {
    type Error = Infallible;

    fn is_open(&self) -> bool {
        self.remaining_frames > 0
    }

    fn poll_events(&mut self) -> Result<(), Self::Error> {
        self.event_polls += 1;
        Ok(())
    }

    fn present_frame(&mut self, surface: &[u32]) -> Result<(), Self::Error> {
        self.last_frame.clear();
        self.last_frame.extend_from_slice(surface);
        self.remaining_frames -= 1;
        Ok(())
    }
}

#[test]
fn frame_presentation_loop_runs_headless_without_panicking() {
    let mut emulator = Emulator::new();
    let mut presenter = HeadlessPresenter::new(1);

    let frames = run_emulation_loop(&mut emulator, &mut presenter, 1_024, Some(1), Some(10_000))
        .expect("headless frame presentation should succeed");

    assert_eq!(frames, 1);
    assert_eq!(presenter.last_frame.len(), latchboy_core::FRAMEBUFFER_LEN);
    assert!(
        presenter
            .last_frame
            .iter()
            .all(|pixel| DMG_PALETTE_RGB.contains(pixel)),
        "all rendered pixels should map to the stable DMG palette"
    );
}

#[test]
fn emulation_loop_can_terminate_without_frame_ready_when_iteration_budget_is_exhausted() {
    let mut emulator = Emulator::new();
    let mut presenter = HeadlessPresenter::new(1);

    let frames = run_emulation_loop(&mut emulator, &mut presenter, 4, Some(1), Some(8))
        .expect("iteration budget should allow clean exit without panicking");

    assert_eq!(frames, 0);
    assert!(presenter.last_frame.is_empty());
}

#[test]
fn blit_maps_all_shades_into_expected_palette_entries() {
    let src = [0, 1, 2, 3]
        .into_iter()
        .cycle()
        .take(latchboy_core::FRAMEBUFFER_LEN)
        .collect::<Vec<_>>();
    let mut dst = vec![0u32; latchboy_core::FRAMEBUFFER_LEN];

    blit_dmg_framebuffer_to_rgb_surface(&src, &mut dst)
        .expect("blit should accept correct framebuffer and surface sizes");

    assert_eq!(dst[0], DMG_PALETTE_RGB[0]);
    assert_eq!(dst[1], DMG_PALETTE_RGB[1]);
    assert_eq!(dst[2], DMG_PALETTE_RGB[2]);
    assert_eq!(dst[3], DMG_PALETTE_RGB[3]);
}

#[test]
fn emulation_loop_rejects_zero_cycle_step() {
    let mut emulator = Emulator::new();
    let mut presenter = HeadlessPresenter::new(1);

    let error = run_emulation_loop(&mut emulator, &mut presenter, 0, Some(1), Some(8))
        .expect_err("zero cycle step should be rejected");
    assert_eq!(error.to_string(), "cycle_step must be greater than zero");
}

#[test]
fn emulation_loop_chunks_large_cycle_steps_to_avoid_dropping_frame_ready_pulses() {
    let mut emulator = Emulator::new();
    let mut presenter = HeadlessPresenter::new(3);

    let frames = run_emulation_loop(&mut emulator, &mut presenter, 210_000, Some(3), Some(1_000))
        .expect("large cycle-step run should complete");

    assert_eq!(frames, 3);
    assert_eq!(presenter.last_frame.len(), latchboy_core::FRAMEBUFFER_LEN);
}

#[test]
fn emulation_loop_polls_events_while_presenting_frames() {
    let mut emulator = Emulator::new();
    let mut presenter = HeadlessPresenter::new(2);

    let frames = run_emulation_loop(&mut emulator, &mut presenter, 1_024, Some(2), Some(20_000))
        .expect("run with event polling enabled should complete");

    assert_eq!(frames, 2);
    assert!(
        presenter.event_polls > 0,
        "event polling should be exercised during frame loop"
    );
}

#[test]
fn emulation_loop_drains_pending_frame_ready_before_stepping() {
    let mut emulator = Emulator::new();
    emulator.step_cycles(70_224);
    assert!(
        emulator.take_frame_ready(),
        "precondition: one completed frame should be pending after pre-stepping"
    );
    emulator.step_cycles(70_224);
    let pre_loop_cycles = emulator.total_cycles();

    let mut presenter = HeadlessPresenter::new(1);
    let frames = run_emulation_loop(&mut emulator, &mut presenter, 210_000, Some(1), Some(100))
        .expect("pending frame-ready should be presented before stepping");

    assert_eq!(frames, 1);
    assert_eq!(
        emulator.total_cycles(),
        pre_loop_cycles,
        "loop should not advance cycles before presenting pending frame-ready data"
    );
}

#[test]
fn emulation_loop_stops_immediately_when_poll_requests_close() {
    let mut emulator = Emulator::new();
    let pre_loop_cycles = emulator.total_cycles();
    let mut presenter = CloseOnPollPresenter::new();

    let frames = run_emulation_loop(&mut emulator, &mut presenter, 210_000, Some(3), Some(1_000))
        .expect("close-on-poll should terminate cleanly");

    assert_eq!(frames, 0);
    assert_eq!(presenter.presents, 0);
    assert_eq!(
        emulator.total_cycles(),
        pre_loop_cycles,
        "loop should not step cycles after a close request from poll_events"
    );
}

#[test]
fn emulation_loop_honors_frame_limit_before_polling_again() {
    let mut emulator = Emulator::new();
    let mut presenter = PollErrorAfterSinglePresent::new();

    let frames = run_emulation_loop(&mut emulator, &mut presenter, 1_024, Some(1), Some(20_000))
        .expect("frame-limit completion should return before next poll_events call");

    assert_eq!(frames, 1);
}
