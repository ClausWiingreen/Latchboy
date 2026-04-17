use std::convert::Infallible;

use latchboy_core::Emulator;
use latchboy_desktop::{
    blit_dmg_framebuffer_to_rgb_surface, run_emulation_loop, FramePresenter, DMG_PALETTE_RGB,
};

struct HeadlessPresenter {
    remaining_frames: u64,
    last_frame: Vec<u32>,
}

impl HeadlessPresenter {
    fn new(frame_budget: u64) -> Self {
        Self {
            remaining_frames: frame_budget,
            last_frame: Vec::new(),
        }
    }
}

impl FramePresenter for HeadlessPresenter {
    type Error = Infallible;

    fn is_open(&self) -> bool {
        self.remaining_frames > 0
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
