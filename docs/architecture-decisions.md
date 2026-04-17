# Game Boy Emulator — Architecture Decisions

## Decision Summary

- **Implementation language:** Rust (stable toolchain).
- **Core architecture style:** Modular core crate with strict subsystem boundaries.
- **Execution model:** **Cycle-stepped emulation loop** (single source of truth for time), with instruction decoding/execution consuming cycles.

Date: 2026-04-13

---

## 1) Implementation Language Choice

### Decision
Use **Rust** as the primary implementation language for the emulator core and initial desktop frontend integration.

### Why Rust

- **Performance with safety:** Emulator hot paths (CPU/PPU/APU/timer) can run near C/C++ performance while keeping memory safety guarantees.
- **Good fit for state machines:** Hardware components are explicit mutable state machines; Rust’s structs/enums/traits model this cleanly.
- **Strong test ergonomics:** Built-in unit/integration testing and deterministic behavior are useful for ROM-based validation.
- **Long-term maintainability:** Compiler checks reduce regressions in timing-sensitive refactors.

### Tradeoffs Accepted

- Learning curve and borrow-checker complexity during initial development.
- Slightly longer prototyping time vs scripting languages.

---

## 2) Core Module Architecture

### Decision
Define the emulator around these core modules:

- `cartridge` — ROM parsing, header metadata, MBC dispatch, external RAM save loading/saving.
- `bus` — Address decoding and routing reads/writes across memory-mapped devices.
- `cpu` — LR35902 register file, opcode decode/execute, interrupt entry/exit, HALT/STOP handling.
- `interrupts` — IF/IE/IME policy and interrupt prioritization rules.
- `timer` — DIV/TIMA/TMA/TAC behavior and overflow interrupt generation.
- `ppu` — Scanline/mode timing, tile/sprite fetch rules, LCD register handling, framebuffer generation.
- `apu` — Frame sequencer, channel generation/mixing, sample buffering.
- `input` — Joypad matrix state and interrupt signaling.
- `serial` — Serial register behavior and optional debug transfer hook.
- `frontend` — Platform adapters (window/audio/input), frame presentation, user controls.

### Architectural Boundaries

- `core` owns emulation truth and is frontend-agnostic.
- `frontend` is an adapter layer that drives the core clock and maps user/system I/O.
- `bus` is the only module with global address-map awareness.
- Subsystems expose minimal read/write/tick APIs to keep deterministic interactions explicit.

---

## 3) Execution Model Choice

### Decision
Use a **cycle-stepped core**:

1. CPU executes an instruction and returns cycles consumed.
2. Timer/PPU/APU/serial/DMA are advanced by those cycles.
3. Interrupt requests are latched/resolved according to hardware ordering.
4. Frontend consumes completed frames/samples from queues.

### Why cycle-stepped

- Best alignment with Game Boy timing quirks (PPU modes, timer edge behavior, DMA impact).
- Avoids drift between subsystems that often appears with purely instruction-stepped emulation.
- Makes tricky test ROM failures easier to diagnose because all components share one clock.

### Consequences

- Slightly more implementation complexity in scheduler and tick APIs.
- Higher initial effort, but lower long-term compatibility cost.

---

## 4) Immediate Implementation Notes

- Start with a `core` crate and enforce no frontend dependencies inside it.
- Keep a central `Emulator` state container responsible for:
  - reset/boot mode selection,
  - stepping cycles,
  - exposing frame/audio output events,
  - save-state hooks (future).
- Define a stable interface contract early for `read8(addr)`/`write8(addr, val)` and component `tick(cycles)`.

---

## 5) Revisit Triggers

Re-evaluate these decisions if any of the following occur:

- Persistent inability to hit target real-time performance on baseline hardware.
- Need for multiple frontends requiring stricter plugin boundaries.
- CGB timing support reveals architecture-level limitations in scheduler granularity.

---

## 6) PPU Framebuffer Ownership + Frame Boundary Contract

### Decision
Expose a deterministic, PPU-owned framebuffer from the core API:

- Dimensions are fixed at **160x144** (`FRAMEBUFFER_WIDTH`, `FRAMEBUFFER_HEIGHT`).
- Pixel format is **DMG shade index bytes** (`0..=3`) in row-major order.
- `take_frame_ready()` emits **exactly one pulse per completed frame** at the VBlank-entry
  boundary after all visible scanlines have been composited into the framebuffer.
- Disabling LCD (`LCDC.7 = 0`) clears the framebuffer to shade `0` (blank/white), matching
  per-pixel APIs that return blank while LCD is off.

### API/ownership contract

- Storage is owned internally by the PPU for the lifetime of the emulator instance.
- Frontends receive a read-only borrow (`&[u8]`) and **must copy** if they need to hold frame
  data across future mutable emulator stepping calls.
- This keeps ownership simple, avoids per-frame allocations, and guarantees deterministic
  visibility of the latest completed frame.

### Rendering flow

- During each visible scanline, pixels are composited (BG/window + sprite rules) and written into
  the framebuffer when that scanline exits Mode 3 (pixel-transfer) into Mode 0 (HBlank).
- By the time scanline 143 completes and VBlank starts, the framebuffer contains a coherent full
  frame ready for frontend presentation.
