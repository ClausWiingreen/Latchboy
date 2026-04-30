# Frame Output API Contract (Single Source of Truth)

This document is the canonical frame-output contract for emulator-to-frontend integration.

## Canonical producer API (`latchboy-core`)

The **only** canonical producer surface for rendered DMG frames is exposed by `latchboy-core`:

- Dimensions/constants:
  - `FRAMEBUFFER_WIDTH = 160`
  - `FRAMEBUFFER_HEIGHT = 144`
  - `FRAMEBUFFER_LEN = FRAMEBUFFER_WIDTH * FRAMEBUFFER_HEIGHT`
- Frame access:
  - `Emulator::framebuffer_pixels() -> &[u8]`
  - `Emulator::take_frame_ready() -> bool`

Pixel semantics of the core framebuffer are fixed:

- Buffer layout: row-major (`index = y * 160 + x`).
- Element type: `u8` DMG shade index.
- Shade encoding:
  - `0` = white
  - `1` = light gray
  - `2` = dark gray
  - `3` = black

Ownership/lifetime:

- The backing storage is owned by `Ppu`.
- Consumers may only borrow immutable slices and must copy/snapshot if data is needed beyond mutable emulator stepping.

## Canonical consumer mapping (`latchboy-desktop`)

Frontend conversion from DMG shade bytes to display pixels must go through:

- `blit_dmg_framebuffer_to_rgb_surface(framebuffer: &[u8], surface: &mut [u32])`

Mapping semantics are fixed:

- Input: DMG shade indices (`0..=3`).
- Output: RGB888 packed as `0x00RRGGBB`.
- Palette source: `DMG_PALETTE_RGB`.

## Non-goals / prohibited divergence

To keep later milestones consistent, avoid introducing duplicate ad-hoc frame APIs that redefine:

- frame dimensions,
- frame stride/layout,
- shade encoding,
- or frame-ready signaling semantics.

Any platform adapter (desktop/web/mobile) should treat this document + the exported core constants as normative.
