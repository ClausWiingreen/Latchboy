# Emulator Validation Tests

This directory is reserved for ROM-based integration validation, deterministic headless harnesses, and golden output fixtures.

## Current deterministic harness

A deterministic headless harness scaffold lives in the `latchboy-core` test suite and can be executed with:

```bash
cargo test -p latchboy-core --test headless_harness
```

The harness currently validates that cycle stepping and reset behavior are reproducible,
using a test-side hash of observable emulator state (`total_cycles`) for deterministic assertions.

## External ROM validation flow

Milestone 3.5 adds a ROM manifest consumed by `core/tests/external_rom_validation.rs`:

- Manifest path: `tests/rom_manifest.toml`
- ROM root source: `$LATCHBOY_ROM_ROOT`
- Runner command:

```bash
LATCHBOY_ROM_ROOT=/path/to/roms cargo test -p latchboy-core --test external_rom_validation
```

### Manifest format

Each `[[roms]]` entry defines deterministic execution budgets:

- `id`: stable ROM case identifier.
- `suite`: suite grouping (`blargg_cpu_instrs`, `blargg_instr_timing`, `mooneye_acceptance_cpu`).
- `path`: ROM path relative to `$LATCHBOY_ROM_ROOT`.
- `milestone`: backlog milestone gating (Milestone 2 entries are CI-required in external validation flow).
- `required`: whether the case must pass for milestone gate checks.
- `cycle_limit`: absolute cycle budget.
- `frame_limit`: frame budget (runner uses 70,224 cycles/frame).
- `wall_time_limit_ms`: runtime wall-time budget per case.

The runner executes each required Milestone 2 entry with deterministic cycle stepping, fails on unimplemented opcode dispatch, and fails when any required case exceeds its time/cycle/frame budget.
