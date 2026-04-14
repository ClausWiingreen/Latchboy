# Game Boy Emulator Implementation Backlog

## Goal
Build a reliable, testable, and reasonably accurate Nintendo Game Boy (DMG) emulator with a clear path to add Game Boy Color (CGB) support later.

---

## Milestone 0 — Project Setup & Foundations

- [x] **Choose implementation language and architecture**
  - [x] Document decisions in `docs/architecture-decisions.md`.
  - [x] Define core modules: `cartridge`, `cpu`, `ppu`, `apu`, `bus`, `timer`, `input`, `serial`, `interrupts`, `frontend`.
  - [x] Decide execution model (cycle-stepped core vs instruction-stepped with cycle accounting).
- [x] **Create repository structure**
  - [x] `core/` for emulation logic.
  - [x] `platform/` for SDL/Web/native frontends.
  - [x] `tests/` for ROM-based validation.
  - [x] `docs/` for architecture and hardware notes.
- [x] **Developer tooling**
  - [x] Add formatter, linter, and CI pipeline.
  - [x] Set up deterministic test harness for headless runs.
- [x] **Reference & legal notes**
  - [x] Document acceptable ROM usage policy.
  - [x] Add links to public hardware docs and test ROM suites.

**Acceptance criteria**
- [x] Clean build in CI.
- [x] Modules compile with stubs and interface contracts.

---

## Milestone 1 — Cartridge & Memory Subsystem

- [ ] **ROM loading**
  - [ ] Parse cartridge header (title, type, ROM/RAM size, destination).
  - [ ] Validate header checksum and expose warnings.
  - [x] Add unit tests for representative header variants (ROM-only, MBC1, MBC3, MBC5).
- [x] **ROM loading**
  - [x] Parse cartridge header (title, type, ROM/RAM size, destination).
  - [x] Validate header checksum and expose warnings.
  - [x] Add unit tests for representative header variants (ROM-only, MBC1, MBC3, MBC5).
- [ ] **Memory Bank Controllers (MBC)**
  - [x] Implement ROM-only (no MBC).
  - [ ] Implement MBC1.
  - [ ] Implement MBC3 (RTC optional phase split).
  - [ ] Implement MBC5.
- [ ] **External RAM handling**
  - [ ] RAM enable/disable behavior.
  - [ ] Battery-backed save persistence (`.sav`).
- [ ] **Address bus mapping**
  - [ ] Map all DMG address ranges and mirroring (including WRAM echo and unusable regions).
  - [ ] Correctly route reads/writes between components.
  - [ ] Add FF50 boot ROM disable register behavior hook.

**Acceptance criteria**
- ROM-only games boot into code execution.
- MBC bank switching passes targeted unit/integration tests.

---

## Milestone 2 — CPU Core (Sharp LR35902)

- [ ] **Register model and flags**
  - [ ] AF, BC, DE, HL, SP, PC.
  - [ ] Accurate Z/N/H/C flag behavior per instruction.
- [ ] **Instruction decoder + executor**
  - [ ] Implement base opcode table.
  - [ ] Implement CB-prefixed table.
  - [ ] Handle invalid/unused opcodes safely.
  - [ ] Add table-driven instruction tests for arithmetic, loads, and bit ops.
- [ ] **CPU timing**
  - [ ] Instruction cycle counts.
  - [ ] Memory access timing interactions.
- [ ] **Control flow and stack**
  - [ ] CALL/RET/RETI, JP/JR, RST, PUSH/POP.
- [ ] **Interrupt mechanism**
  - [ ] IME behavior and delayed EI semantics.
  - [ ] IF/IE register interaction.
  - [ ] HALT bug behavior (deferred final-accuracy tuning allowed).

**Acceptance criteria**
- Passes CPU instruction correctness test ROMs.
- Passes interrupt behavior test subset.

---

## Milestone 3 — Timers, Interrupts, and Boot Sequence

- [ ] **DIV/TIMA/TMA/TAC**
  - [ ] Falling-edge timer behavior.
  - [ ] Overflow/reload edge cases and interrupt requests.
- [ ] **Interrupt controller integration**
  - [ ] Priority ordering.
  - [ ] HALT behavior and wake-up behavior.
- [ ] **Boot ROM handling**
  - [ ] Optional boot ROM execution path.
  - [ ] Post-boot register defaults for no-boot mode.
  - [ ] Document exact startup assumptions in code comments/tests.

**Acceptance criteria**
- Timer test ROMs pass.
- Boot/no-boot paths both produce stable startup.

---

## Milestone 4 — PPU (Graphics Pipeline)

- [ ] **VRAM/OAM and LCD registers**
  - [ ] Implement FF40–FF4B behavior.
  - [ ] Enforce mode-based access restrictions where needed.
- [ ] **PPU modes and scanline timing**
  - [ ] Mode 2 (OAM search), Mode 3 (drawing), Mode 0 (HBlank), Mode 1 (VBlank).
  - [ ] LY/LYC compare and STAT interrupt triggers.
- [ ] **Background/window rendering**
  - [ ] Tile fetching and map addressing.
  - [ ] Scroll/window positioning rules.
- [ ] **Sprite rendering**
  - [ ] OAM priority, X/Y offsets, flipping, palette selection.
  - [ ] 8x8 and 8x16 object modes.
- [ ] **Framebuffer output**
  - [ ] DMG 4-shade palette mapping.
  - [ ] VBlank frame-ready signal to frontend.

**Acceptance criteria**
- PPU timing + rendering test ROMs mostly pass.
- Several commercial titles render readable menus/UI.

---

## Milestone 5 — Input and DMA

- [ ] **Joypad input (FF00)**
  - [ ] Button matrix selection and polling.
  - [ ] Joypad interrupt generation.
- [ ] **DMA transfer (FF46)**
  - [ ] OAM DMA timing and CPU bus impact.

**Acceptance criteria**
- Input works consistently in at least 3 games.
- DMA-sensitive sprite behavior is correct in test ROMs.

---

## Milestone 6 — Serial I/O

- [ ] **Serial link registers**
  - [ ] Implement SB/SC read-write behavior.
  - [ ] Basic internal clock transfer stub.
  - [ ] Test hook/log output for serial-based test ROMs.

**Acceptance criteria**
- Blargg/mooneye serial-output ROMs can report pass/fail via serial capture.

---

## Milestone 7 — APU (Audio)

- [ ] **Audio architecture setup**
  - [ ] Frame sequencer implementation.
  - [ ] Sample generation and output buffering.
- [ ] **Channel implementation**
  - [ ] CH1: square + sweep.
  - [ ] CH2: square.
  - [ ] CH3: wave channel.
  - [ ] CH4: noise channel.
- [ ] **Mixer and control registers**
  - [ ] NR50/NR51/NR52 behavior.
  - [ ] Stereo routing and master enable.
- [ ] **Sync with emulation clock**
  - [ ] Avoid underruns and drift.

**Acceptance criteria**
- Audio test ROMs pass core checks.
- Audible playback in sample games with stable pitch/timing.

---

## Milestone 8 — Frontend, UX, and Debug Tooling

- [ ] **Desktop frontend**
  - [ ] Window creation, frame blit, vsync toggle.
  - [ ] Keyboard/gamepad mapping and remapping.
- [ ] **Runtime features**
  - [ ] Save/load state slots.
  - [ ] Fast-forward and frame stepping.
  - [ ] Pause/reset and ROM hot-reload.
- [ ] **Debug tools**
  - [ ] CPU register/memory inspector.
  - [ ] Breakpoints and instruction trace logger.
  - [ ] Tile/OAM debug viewers.

**Acceptance criteria**
- Playable user experience for core DMG titles.
- Debugger usable for diagnosing failing test ROMs.

---

## Milestone 9 — Accuracy & Compatibility Hardening

- [ ] **Test ROM automation**
  - [ ] Integrate Blargg and Mooneye test runs in CI.
  - [ ] Snapshot-based rendering regression tests.
- [ ] **Edge-case behavior fixes**
  - [ ] HALT bug nuances.
  - [ ] STAT interrupt quirks.
  - [ ] Sprite priority corner cases.
- [ ] **Performance profiling**
  - [ ] CPU hotspots.
  - [ ] PPU scanline throughput.
  - [ ] Audio callback stability.

**Acceptance criteria**
- Consistent pass rate across selected official/community suites.
- Full-speed emulation on target baseline hardware.

---

## Milestone 10 — Optional Extensions

- [ ] **Game Boy Color (CGB) support**
  - [ ] Double-speed mode.
  - [ ] CGB palettes and VRAM banking.
  - [ ] CGB-specific registers and boot flow.
- [ ] **Link cable emulation**
  - [ ] Local loopback.
  - [ ] Networked peer mode.
- [ ] **Additional platforms**
  - [ ] WebAssembly build.
  - [ ] Mobile frontend.

**Acceptance criteria**
- CGB boot and basic title compatibility (if in scope).

---

## Cross-Cutting Quality Backlog

- [ ] **Documentation**
  - [ ] Architecture decision records.
  - [ ] Component-level timing diagrams.
  - [ ] Contributor onboarding guide.
- [ ] **Testing strategy**
  - [ ] Unit tests for each subsystem.
  - [ ] Integration tests per hardware event sequence.
  - [ ] Golden tests for known ROM outputs.
- [ ] **Release engineering**
  - [ ] Versioning and changelog automation.
  - [ ] Reproducible builds.
  - [ ] Crash reporting and diagnostics bundle.

---

## Suggested Delivery Order

1. Cartridge + Bus
2. CPU + Interrupt core
3. Timers + Boot
4. PPU + Input + DMA
5. Serial test-output support
6. Frontend minimum playable loop
7. APU
8. Compatibility hardening
9. Optional CGB/link enhancements

---

## Definition of Done (Project-Level)

- [ ] Passes agreed CPU/PPU/timer test ROM baseline.
- [ ] Boots and plays a curated compatibility set.
- [ ] Maintains full-speed emulation on target platform.
- [ ] Ships with save support, configurable controls, and basic debugging tools.
- [ ] Includes clear documentation for architecture and contribution.
