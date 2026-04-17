use latchboy_core::Emulator;
use latchboy_desktop::{present_latest_frame, FrameSurface, DMG_PALETTE_ARGB8888};

struct FakeSurface {
    last_width: usize,
    last_height: usize,
    updates: usize,
}

impl FrameSurface for FakeSurface {
    fn blit_argb8888(&mut self, width: usize, height: usize, pixels: &[u32]) -> Result<(), String> {
        self.last_width = width;
        self.last_height = height;
        self.updates += 1;
        assert_eq!(pixels.len(), width * height);
        Ok(())
    }
}

#[test]
fn frame_presentation_path_is_callable_after_frame_ready() {
    let mut emulator = Emulator::new();
    emulator.step_cycles(70_224);
    assert!(emulator.take_frame_ready());

    let mut surface = FakeSurface {
        last_width: 0,
        last_height: 0,
        updates: 0,
    };

    present_latest_frame(&emulator, &mut surface, DMG_PALETTE_ARGB8888)
        .expect("frame presentation should not panic or fail");

    assert_eq!(surface.last_width, 160);
    assert_eq!(surface.last_height, 144);
    assert_eq!(surface.updates, 1);
}
