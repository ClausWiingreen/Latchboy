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

- [x] **ROM loading**
  - [x] Parse cartridge header (title, type, ROM/RAM size, destination).
  - [x] Validate header checksum and expose warnings.
  - [x] Add unit tests for representative header variants (ROM-only, MBC1, MBC3, MBC5).
- [x] **Memory Bank Controllers (MBC)**
  - [x] Implement ROM-only (no MBC).
  - [x] Implement MBC1.
  - [x] Implement MBC3 (RTC optional phase split).
  - [x] Implement MBC5.
- [x] **External RAM handling**
  - [x] RAM enable/disable behavior.
  - [x] Battery-backed save persistence (`.sav`) via platform filesystem integration.
  - [x] In-memory save serialization/deserialization API (`save_data` / `load_save_data`).
- [x] **Address bus mapping**
  - [x] Map all DMG address ranges and mirroring (including WRAM echo and unusable regions).
  - [x] Correctly route reads/writes between components.
  - [x] Add FF50 boot ROM disable register behavior hook.

**Acceptance criteria**
- [x] ROM-only cartridge boot smoke executes code from ROM through the bus and halts deterministically.
- [x] MBC bank switching passes targeted unit/integration tests.
- [x] Battery-backed RAM can round-trip via platform `.sav` persistence for desktop frontend flows.

**Implementation review notes (2026-04-14)**
- Header parsing, warnings, checksum handling, and representative mapper coverage (ROM-only, MBC1, MBC3, MBC5) are implemented with comprehensive unit tests.
- DMG bus mapping is implemented for the full address ranges in this milestone, including WRAM echo mirroring, unusable region behavior, and FF50 boot ROM disable hook behavior.
- Battery-backed RAM now has both in-memory serialization APIs and desktop platform `.sav` load/save plumbing with atomic writes and corruption-size guards.
- A ROM boot smoke path exists and is validated by tests, but full commercial game boot compatibility still depends on Milestones 2–5 (CPU coverage, timers/interrupts, PPU, input, DMA).

---

## Milestone 2 — CPU Core (Sharp LR35902)

- [x] **Register model and flags**
  - [x] AF, BC, DE, HL, SP, PC.
  - [x] Accurate Z/N/H/C flag behavior per instruction.
- [x] **Instruction decoder + executor**
  - [x] Implement base opcode table (current scaffold includes a small subset used by smoke tests).
  - [x] Implement CB-prefixed table.
  - [x] Handle invalid/unused opcodes safely in non-test builds (avoid panic-based control flow).
  - [x] Add table-driven instruction tests for arithmetic, loads, and bit ops.
- [x] **CPU timing**
  - [x] Instruction cycle counts.
  - [x] Memory access timing interactions.
- [x] **Control flow and stack**
  - [x] CALL/RET/RETI, JP/JR, RST, PUSH/POP.
- [x] **Interrupt mechanism**
  - [x] IME behavior and delayed EI semantics.
  - [x] IF/IE register interaction.
  - [x] HALT bug behavior (deferred final-accuracy tuning allowed).

**Implementation review notes (2026-04-14)**
- Base opcode coverage has been expanded beyond the original smoke-test subset to include `LD rr,d16` for all register pairs, `ADC`/`SBC` register forms, and immediate ALU forms (`ADD/ADC/SUB/SBC/AND/XOR/OR/CP d8`).
- Additional base-opcode load/store and 16-bit arithmetic coverage now includes indirect accumulator transfers (`LD (BC)/(DE),A`, `LD A,(BC)/(DE)`, `LD (HL+)/ (HL-),A`, `LD A,(HL+)/ (HL-)`), high-memory variants (`LDH (a8),A`, `LDH A,(a8)`, `LD (C),A`, `LD A,(C)`), absolute accumulator transfers (`LD (a16),A`, `LD A,(a16)`), plus `INC/DEC rr` and `ADD HL,rr`.
- Base-opcode miscellaneous accumulator operations now include non-CB rotates (`RLCA`, `RRCA`, `RLA`, `RRA`) and flag-transforming instructions (`DAA`, `CPL`, `SCF`, `CCF`) with focused unit-test coverage.
- Base opcode control-flow/stack coverage now includes relative and conditional jumps (`JR`, `JR cc`), absolute and conditional jumps/calls (`JP`, `JP cc`, `CALL`, `CALL cc`, `JP (HL)`), returns (`RET`, `RET cc`), stack transfer instructions (`PUSH/POP rr`), restart vectors (`RST`), and SP/HL transfer-family instructions (`LD (a16),SP`, `ADD SP,e8`, `LD HL,SP+e8`, `LD SP,HL`).
- CB-prefixed decode/execute support now covers rotate/shift (`RLC/RRC/RL/RR/SLA/SRA/SWAP/SRL`), bit-test (`BIT`), and bit-manipulation (`RES`/`SET`) instruction groups for both register and `(HL)` targets, including per-target timing differences.
- CPU unit tests now include focused coverage for 16-bit register-pair loads and carry-sensitive ALU behavior for both register and immediate instruction forms.
- Instruction decode/execute coverage is now feature-complete for all implemented base and CB opcode families in this milestone scope; only hardware-invalid opcodes route to diagnostics.
- Invalid/unused opcode dispatch now avoids panic-based control flow by halting execution and recording the offending opcode for diagnostics.
- Remaining valid base opcodes now include `STOP`, `RETI`, `DI`, and `EI` semantics, leaving only hardware-invalid instructions to trigger unimplemented-opcode diagnostics.
- Instruction timing coverage now includes table-driven tests for representative base and CB opcode cycle counts, including branch taken/not-taken paths and stack-return timing differences.
- Additional timing tests now explicitly verify memory-operand penalties against register-operand paths for both base ALU and CB-prefixed operations.
- CPU step sequencing now performs IF/IE pending-interrupt arbitration ahead of opcode fetch, services enabled interrupts by priority when IME is set, and exits HALT state when interrupts are pending.

**Acceptance criteria**
- Passes CPU instruction correctness test ROMs.
- Passes interrupt behavior test subset.

**Acceptance status review (2026-04-16)**
- ✅ Milestone 2 implementation scope is present in-tree: register/flag model, broad base + CB instruction families, control-flow/stack ops, interrupt dispatch, HALT/HALT-bug behavior, and non-panicking invalid-opcode diagnostics.
- ✅ Workspace tests currently pass for CPU correctness and interrupt-focused behavior via `cargo test --workspace --all-targets`.
- ⚠️ External ROM acceptance remains fixture-dependent: `external_rom_validation` skips required ROM runs when `LATCHBOY_ROM_ROOT` is unset/empty, so acceptance is only fully proven when fixtures are mounted in CI/local runs.
- 🔧 Clarification: treat these acceptance bullets as satisfied by the required Milestone 2 `tests/rom_manifest.toml` entries plus CPU interrupt-focused unit/integration tests.

**Milestone 2 completion gate (linked validation docs)**
- Validation runbook + fixture/manifest contract: `tests/README.md`.
- Required CI check run name: `CI / rust-checks` (workflow `.github/workflows/ci.yml`, job `rust-checks`).
- Acceptance bullet mapping:
  - CPU instruction correctness ROM coverage → required Milestone 2 entries in `tests/rom_manifest.toml` exercised by `cargo test --workspace --all-targets` under `CI / rust-checks`.
  - Interrupt behavior subset → CPU interrupt-focused tests in `latchboy-core` within the same workspace test invocation, plus required Milestone 2 external ROM validation entries.

---

## Milestone 3 — Timers, Interrupts, and Boot Sequence

- [x] **DIV/TIMA/TMA/TAC**
  - [x] Falling-edge timer behavior.
  - [x] Overflow/reload edge cases and interrupt requests.
- [x] **Interrupt controller integration**
  - [x] Priority ordering.
  - [x] HALT behavior and wake-up behavior.


**Implementation review notes (2026-04-16)**
- CPU interrupt servicing is integrated into prefetch step sequencing with hardware-priority dispatch (VBlank → LCD STAT → Timer → Serial → Joypad) via lowest-set-bit selection on IF/IE pending state.
- HALT wake-up behavior is covered for both IME-enabled service and IME-disabled wake-without-service paths, including HALT-bug sequencing when interrupts are pending while IME is clear.
- Current coverage is anchored by focused CPU tests (`interrupt_service_uses_hardware_priority_order`, `halted_cpu_wakes_on_pending_interrupt_even_when_ime_is_disabled`, and `halt_bug_repeats_next_opcode_fetch_when_ime_is_disabled_with_pending_interrupt`).

- [ ] **Boot ROM handling**
  - [ ] Optional boot ROM execution path.
  - [ ] Post-boot register defaults for no-boot mode.
  - [ ] Document exact startup assumptions in code comments/tests.

**Acceptance criteria**
- Timer test ROMs pass.
- Boot/no-boot paths both produce stable startup.

---

## Milestone 3.5 — External Validation Harness

- [x] **ROM manifest + loader for headless runs**
  - [x] Add a versioned manifest file in `tests/` for suite registration and per-ROM budgets.
  - [x] Add loader/runner coverage in `core/tests` for deterministic headless execution.
- [x] **Milestone 2 suite registration**
  - [x] Register Blargg `cpu_instrs` coverage.
  - [x] Register Blargg `instr_timing` coverage.
  - [x] Register a Mooneye CPU acceptance subset (currently deferred/non-required).
- [x] **Deterministic execution budgets**
  - [x] Enforce per-ROM cycle/frame/wall-time limits.
- [x] **CI gate behavior**
  - [x] Fail CI (external validation flow) when any required Milestone 2 ROM case fails.

**Refinements for upcoming milestones**
- [ ] Expand the required ROM manifest set as Milestones 3–5 land (timers, interrupt edge-cases, boot behavior, and early PPU/input interactions), while preserving deterministic execution budgets.
- [ ] Document CI fixture provisioning for `LATCHBOY_ROM_ROOT` in contributor docs to avoid false-green local runs that skip external ROM execution.

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
  - [ ] Cartridge save-file management (`.sav`) wired to battery-backed RAM APIs.
  - [ ] Auto-load saves on ROM open and flush saves on shutdown/reset/periodic checkpoint.
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

1. Cartridge + Bus + save-data plumbing baseline
2. CPU core (full opcode coverage + deterministic timing scaffolding)
3. External validation harness + CI gate hardening (Milestone 3.5, already landed; continue extending with each milestone)
4. Timers + interrupt controller integration + boot/no-boot startup defaults
5. PPU timing + DMA + input integration (first visual/playability milestone; implement DMA alongside early PPU work even though listed under Milestone 5)
6. Serial test-output support (to unlock Blargg/Mooneye automation feedback loops)
7. Frontend minimum playable loop hardening (controls UX, `.sav` lifecycle checkpoints, reset/hot-reload behavior)
8. APU
9. Compatibility hardening and CI automation of ROM suites
10. Optional CGB/link/platform enhancements

---

## Definition of Done (Project-Level)

- [ ] Passes agreed CPU/PPU/timer test ROM baseline.
- [ ] Boots and plays a curated compatibility set.
- [ ] Maintains full-speed emulation on target platform.
- [ ] Ships with save support, configurable controls, and basic debugging tools.
- [ ] Includes clear documentation for architecture and contribution.
