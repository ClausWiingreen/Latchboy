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

- [x] **Boot ROM handling**
  - [x] Optional boot ROM execution path.
  - [x] Post-boot register defaults for no-boot mode.
  - [x] Document exact startup assumptions in code comments/tests.

**Implementation review notes (2026-04-16)**
- Emulator startup now has two explicit, tested paths: DMG post-boot initialization (`from_cartridge`) and mapped boot-ROM execution (`from_cartridge_with_boot_rom`), with comments describing each path’s assumptions.
- No-boot startup defaults are covered by assertions on CPU register state and key I/O defaults, including `PC=0x0100`, `SP=0xFFFE`, and `FF50=0x01`.
- Boot-ROM startup execution and unmapping behavior are validated end-to-end through `FF50`, and reset behavior now re-establishes the correct initial state for both startup modes.

**Acceptance criteria**
- Timer test ROMs pass.
- Boot/no-boot paths both produce stable startup.

**Acceptance status review (2026-04-20, updated)**
- ✅ `Timer test ROMs pass` now has **manifest-level coverage wiring** in-tree: `tests/rom_manifest.toml` includes required Milestone 3 timer-adjacent entries (`blargg-instr-timing-m3-gate`, `mooneye-acceptance-timer-div-write`) plus an explicit deferred timer case (`mooneye-acceptance-timer-rapid-toggle`), and `external_rom_validation` enforces this structure.
- ⚠️ `Timer test ROMs pass` is still **fixture-execution dependent** for final sign-off: required Milestone 2/3 ROM run tests skip when `LATCHBOY_ROM_ROOT` is unset/empty, so green workspace runs without fixtures are not sufficient closure evidence.
- ✅ `Boot/no-boot paths both produce stable startup` is satisfied by in-tree tests covering DMG post-boot defaults, explicit boot ROM execution/unmapping via `FF50`, and reset behavior across startup modes.
- 🔧 Remaining Milestone 3 closure refinement: add at least one explicit **required boot-path external ROM smoke entry** (or equivalent deterministic external harness case) so startup acceptance has both unit-level and fixture-level evidence, not only unit tests.

**Milestone 3 completion gate (updated)**
- Required ROM classes in `tests/rom_manifest.toml`:
  - Blargg `instr_timing` timer-adjacent coverage (required, milestone = 3) ✅ wired.
  - Mooneye timer/interrupt edge-case subset (required + deferred split, milestone = 3; no-PPU dependency set) ✅ wired.
  - Boot path smoke case(s) (required, milestone = 3; pass signal documented in manifest comments) ⚠️ still missing explicit manifest entry.
- Required CI evidence:
  - `cargo test --workspace --all-targets` with `LATCHBOY_ROM_ROOT` set in the CI environment that runs external ROM validation.
  - CI should fail milestone-gating jobs when `LATCHBOY_ROM_ROOT` is unset/empty (avoid skip-based false green).
  - Green check for `CI / rust-checks` on the target commit.

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
- [x] Expand the required ROM manifest set as Milestones 3–5 land (timers, interrupt edge-cases, boot behavior, and early PPU/input interactions), while preserving deterministic execution budgets.
- [x] Document CI fixture provisioning for `LATCHBOY_ROM_ROOT` in contributor docs to avoid false-green local runs that skip external ROM execution.

## Milestone 4 — PPU (Graphics Pipeline)

- [ ] **VRAM/OAM and LCD registers**
  - [x] Implement FF40–FF4B behavior.
  - [x] Enforce mode-based access restrictions where needed.
- [ ] **PPU modes and scanline timing**
  - [x] Mode 2 (OAM search), Mode 3 (drawing), Mode 0 (HBlank), Mode 1 (VBlank).
  - [x] LY/LYC compare and STAT interrupt triggers.
- [ ] **Background/window rendering**
  - [x] Tile fetching and map addressing.
  - [x] Scroll/window positioning rules.
- [ ] **Sprite rendering**
  - [x] OAM priority, X/Y offsets, flipping, palette selection.
  - [x] 8x8 and 8x16 object modes.
- [ ] **OAM DMA dependency (moved earlier from Milestone 5)**
  - [x] Implement DMA transfer register `FF46`.
  - [x] Model CPU bus contention/timing impact during DMA.
  - [x] Add targeted tests for sprite fetch correctness under DMA activity.
- [ ] **Framebuffer output**
  - [x] DMG 4-shade palette mapping.
  - [x] VBlank frame-ready signal to frontend.

**Implementation review notes (2026-04-17)**
- Core PPU implementation covers milestone building blocks in-tree: LCD register surface (`FF40–FF4B`), mode stepping (2/3/0/1), LY/LYC + STAT edge behavior, background/window tile fetch rules, sprite selection/priority/flip handling (including 8x16), and DMG palette shade mapping.
- OAM DMA is integrated in the bus and already moved to this milestone scope: writing `FF46` performs a 160-byte transfer, models CPU bus blocking (except HRAM) for the DMA contention window, and has targeted unit-test coverage for bus blocking and sprite visibility under DMA writes.
- A concrete framebuffer contract now exists in `core`: a row-major 160x144 DMG shade buffer owned by the PPU, with explicit frame-ready pulse semantics and desktop-side RGB blit integration in `platform/desktop`.
- `tests/rom_manifest.toml` now includes required Milestone 4 Mooneye PPU entries (mode timing + STAT behavior), plus explicit deferred non-required cases for still-in-progress edge behavior.
- In-tree desktop presentation is now a minimal frame loop (headless-friendly surface presenter) rather than a pure scaffold; this supports deterministic frame-ready consumption but does not yet provide interactive window/input UX.

**Acceptance criteria**
- **PPU ROM gate threshold (objective):** Required Milestone 4 PPU manifest entries (`milestone = 4`, `required = true`) pass **100%** under configured fixture runs (`LATCHBOY_ROM_ROOT` set) in `external_rom_validation` (run-level gate test: `required_milestone_4_roms_pass_under_external_validation_flow`).
- **Curated title menu checkpoint threshold (objective):** **3/3** curated commercial-title smoke cases (`tetris-world`, `super-mario-land-world`, `legend-of-zelda-links-awakening-world`) reach their named menu checkpoints within their fixed frame/time budgets.
- **Evidence linkage (must exist for sign-off):**
  - Automated gate evidence: `external_rom_validation` output for `required_milestone_4_roms_pass_under_external_validation_flow`.
  - Smoke evidence artifact: `tests/artifacts/milestone4-smoke-summary.json` (validated against `tests/artifacts/milestone4-smoke-summary.schema.json`).
- **Deterministic verification commands (CI/local):**
  - `LATCHBOY_ROM_ROOT=<rom-fixtures> cargo test -p latchboy-core --test external_rom_validation required_milestone_4_roms_pass_under_external_validation_flow`
  - `cargo run -p latchboy-desktop --bin milestone4_smoke -- --rom <absolute-rom-path> --rom-id <rom-id> --title-id <tetris-world|super-mario-land-world|legend-of-zelda-links-awakening-world> --title-signal-hash <expected-hash> --output-dir tests/artifacts/smoke/milestone4/<timestamp>/<title-id>`

**Acceptance status review (2026-04-18, updated)**
- ⚠️ `Required Milestone 4 PPU manifest entries pass 100%` remains **partially evidenced**. The gate test (`required_milestone_4_roms_pass_under_external_validation_flow`) is implemented and green in normal workspace test runs, but it intentionally skips when `LATCHBOY_ROM_ROOT` is unset/empty; therefore, a green run without fixtures is not sufficient sign-off evidence.
- ⚠️ `3/3 curated titles reach named menu checkpoints within fixed budgets` is **still open**. The smoke harness (`milestone4_smoke`) and schema file are present, but no committed `tests/artifacts/milestone4-smoke-summary.json` evidence file currently exists.
- ⚠️ Schema enforcement for smoke evidence is **still open**. The schema exists, but there is no dedicated automated validation test that fails when a committed milestone summary drifts from `tests/artifacts/milestone4-smoke-summary.schema.json`.
- 🔧 Remaining Milestone 4 closure items:
  - Enforce fixture-backed execution of `required_milestone_4_roms_pass_under_external_validation_flow` in CI for target commits (no skipped external ROM runs).
  - Add and maintain committed smoke evidence at `tests/artifacts/milestone4-smoke-summary.json` (schema-conformant) for curated title IDs (`tetris-world`, `super-mario-land-world`, `legend-of-zelda-links-awakening-world`) (per title: `run.json` fields `commit_sha`/`rom_id`/`runner_command`/`frame_limit`/`wall_time_limit_ms`; `summary.json` fields `status`/`checkpoint_frame_index`/`pass_fail_reason`; plus `hash_window` fields `algorithm`/`start_frame`/`frame_count`/`sample_stride`/`hashes`) for Milestone 4 closure.
  - Add an automated schema-validation gate in tests/CI that validates the committed smoke summary against `tests/artifacts/milestone4-smoke-summary.schema.json`.
  - Explicitly forbid committing copyrighted commercial frame/image/video captures (`final_frame.png`, `frames/`, raw video) in repository history, PR attachments, or public CI artifacts.
  - Keep commercial title readability gating tied to the interactive desktop presentation path (window + event polling/input plumbing) rather than headless-only frame pumps.

---

## Milestone 5 — Input and UX integration

_Note: OAM DMA was intentionally moved into Milestone 4 because sprite correctness/timing depends on it._

- [ ] **Joypad input (FF00)**
  - [ ] Button matrix selection and polling.
  - [ ] Joypad interrupt generation.

**Acceptance criteria**
- Input works consistently in at least 3 games.
- Joypad interrupt behavior passes targeted ROM/unit tests and works consistently in at least 3 games.

---

## Backlog sequencing refinements (2026-04-17)

- [ ] **Priority 1: Tighten milestone-to-validation mapping**
  - [ ] For each milestone from 3 onward, define at least one required external ROM suite entry (`tests/rom_manifest.toml`) before marking the milestone complete.
  - [ ] Keep deferred/non-required entries explicit, with a note describing the dependency (e.g., PPU mode timing not yet in scope).
- [ ] **Priority 1: Close the “registered vs enforced” validation gap**
  - [ ] Ensure every `required = true` manifest entry is covered by at least one test assertion (parser gate + execution path), not only by convention/docs.
  - [ ] Add milestone-scoped gate tests incrementally (`required_milestone_4_*`, then Milestone 5, etc.) so new required suites cannot silently be ignored.
- [ ] **Priority 1: Turn Milestone 4 evidence from “defined” to “enforced”**
  - [ ] Commit `tests/artifacts/milestone4-smoke-summary.json` for the curated title set using metadata + hash-only evidence.
  - [ ] Add a test/CI check that validates committed smoke summary files against `tests/artifacts/milestone4-smoke-summary.schema.json`.
  - [ ] Ensure CI treats missing/empty `LATCHBOY_ROM_ROOT` as a failing configuration for milestone-gating jobs.
- [ ] **Priority 2: Normalize acceptance criteria wording**
  - [ ] Convert broad terms like “mostly pass” and “playable user experience” into measurable checkpoints (required ROM pass %, deterministic budget caps, and minimum smoke-test title list).

## Backlog sequencing refinements (2026-04-20, review-driven updates)

- [ ] **Priority 0: Close Milestone 3 objective evidence gap before expanding milestone scope**
  - [ ] Add a required Milestone 3 boot/startup external validation case to `tests/rom_manifest.toml` (or an equivalent required deterministic external harness case) so startup acceptance is fixture-backed, not unit-only.
  - [ ] Add a dedicated Milestone 3 gate test in `core/tests/external_rom_validation.rs` (`required_milestone_3_roms_pass_under_external_validation_flow`) to avoid coupling Milestone 3 pass/fail status to Milestone 2 runs.
- [ ] **Priority 1: Make fixture-backed milestone gates non-skippable in CI**
  - [ ] Add an explicit CI preflight check that fails when `LATCHBOY_ROM_ROOT` is unset/empty for milestone-gating jobs.
  - [ ] Keep developer-local runs skippable, but make release/merge gate jobs fixture-mandatory and documented as such.
- [ ] **Priority 1: Reorder validation progression to match dependency risk**
  - [ ] Complete Milestone 3 fixture-enforced closure first.
  - [ ] Then complete Milestone 4 PPU required ROM gate + smoke summary schema enforcement.
  - [ ] Only then mark Milestone 5 input/UX acceptance items as active closure targets.
- [ ] **Priority 2: Clarify “evidence of completion” artifacts**
  - [ ] Require a lightweight `tests/artifacts/README.md` schema for which non-copyrighted evidence files must be committed per milestone (for example: manifest diff, hash summaries, pass/fail tables).
  - [ ] Distinguish “implemented”, “documented”, and “gated in CI/tests” status markers so milestone reviews cannot conflate them.
- [ ] **Priority 3: Document blocking dependencies directly inside milestones**
  - [ ] Keep DMA listed under Milestone 4 because sprite correctness/timing depends on it.
  - [ ] Keep serial-output hooks referenced in Milestones 3–5 test plans so Blargg-style pass/fail reporting is available before full serial-link completion.
- [ ] **Priority 3: Re-scope Milestone 5 naming and order**
  - [ ] Rename Milestone 5 heading from “Input and DMA” to “Input and UX integration” (DMA has already been pulled into Milestone 4 implementation scope).
  - [ ] Keep FF00/joypad interrupt behavior as Milestone 5 gate prerequisites before adding broader game-compatibility smoke goals.
  - [ ] Add a follow-on milestone note for post-input interactive goals (pause/reset hotkeys, deterministic input playback, and expanded compatibility smoke list).
- [ ] **Milestone 4 closure contract**
  - [x] Add required PPU-focused ROM entries (`milestone = 4`, `required = true`) before marking Milestone 4 complete.
  - [ ] Gate those required Milestone 4 entries in `external_rom_validation` tests and CI execution.
  - [x] Record a minimum commercial-title smoke matrix (title, expected menu state, deterministic timeout budget, pass signal) in `tests/README.md`.
  - [ ] Require committed smoke summary evidence matching `tests/artifacts/milestone4-smoke-summary.schema.json` (title → `run.json`, `summary.json`, `hash_window`, checkpoint frame index, pass/fail reason) for milestone sign-off.
  - [ ] Enforce policy that only metadata + hashes are committed; copyrighted commercial frame/image/video assets are forbidden.
  - [ ] Define a single source of truth for frame output API (core buffer format + frontend consumption expectations) to avoid duplicated rendering glue in later milestones.

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
5. PPU timing + DMA correctness + framebuffer contract lock-in (close Milestone 4 with required PPU validation before claiming playability)
6. Input (`FF00`) + frontend minimum playable loop hardening (controls UX, frame presentation, `.sav` lifecycle checkpoints, reset/hot-reload behavior)
7. Serial test-output support (to unlock broader Blargg/Mooneye automation feedback loops as compatibility grows)
8. APU
9. Compatibility hardening and CI automation of ROM suites (including curated commercial smoke matrix)
10. Optional CGB/link/platform enhancements

---

## Definition of Done (Project-Level)

- [ ] Passes agreed CPU/PPU/timer test ROM baseline.
- [ ] Boots and plays a curated compatibility set.
- [ ] Maintains full-speed emulation on target platform.
- [ ] Ships with save support, configurable controls, and basic debugging tools.
- [ ] Includes clear documentation for architecture and contribution.
