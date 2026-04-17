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
- `suite`: suite grouping (`blargg_cpu_instrs`, `blargg_instr_timing`, `mooneye_acceptance_cpu`, `mooneye_acceptance_timer`).
- `path`: ROM path relative to `$LATCHBOY_ROM_ROOT`.
- `milestone`: backlog milestone gating (required entries through Milestone 4 are CI-required in external validation flow).
- `required`: whether the case must pass for milestone gate checks.
- `cycle_limit`: absolute cycle budget.
- `frame_limit`: frame budget (runner uses 70,224 cycles/frame).
- `wall_time_limit_ms`: runtime wall-time budget per case.
- `pass_condition`: suite-specific success signal (`blargg_mem` or `mooneye_registers`).

The runner treats unset **or empty** `LATCHBOY_ROM_ROOT` as disabled and skips external ROM execution in that environment.
When enabled, it executes each required Milestone 2/3/4 entry with deterministic cycle stepping, fails on unimplemented opcode dispatch, and fails when a required case does not positively report pass before exceeding its time/cycle/frame budget.

Milestone 3 expands the manifest with required timer-focused entries, while keeping explicit deferred cases (`required = false`) when dependencies are still in progress.

## Local fixture layout for Milestone 2/3/4 ROM validation

Local ROM fixtures must be mounted under a directory referenced by `LATCHBOY_ROM_ROOT`.
The layout below is required for the current checked-in `tests/rom_manifest.toml` entries:

```text
$LATCHBOY_ROM_ROOT/
├── blargg/
│   ├── cpu_instrs/
│   │   └── individual/
│   │       └── 01-special.gb
│   └── instr_timing/
│       └── instr_timing.gb
└── mooneye/
    └── acceptance/
        ├── add_sp_e_timing.gb
        ├── call_cc_timing.gb
        ├── jp_cc_timing.gb
        └── timer/
            ├── div_write.gb
            └── rapid_toggle.gb
```

This tree shows the full currently registered manifest surface (required + deferred).
If any **required** ROM is missing from these paths, `external_rom_validation` fails when ROM validation is enabled.

Milestone 3 uses `mooneye/acceptance/timer/div_write.gb` as a required timer edge-case gate.
`mooneye/acceptance/timer/rapid_toggle.gb` is intentionally deferred (`required = false`) until tighter edge-case behavior is in scope.

Milestone 4 currently tracks deferred CPU-adjacent Mooneye timing fixtures (`add_sp_e_timing.gb`, `call_cc_timing.gb`, `jp_cc_timing.gb`) in the manifest.

## Required Milestone 4 fixture paths (source of truth)

Do **not** infer required Milestone 4 fixtures from the full tree above. The source of truth is
`tests/rom_manifest.toml` entries with both:

- `milestone = 4`
- `required = true`

At the time of writing, the required Milestone 4 gate is the Mooneye PPU acceptance subset
(`mooneye/acceptance/ppu/*.gb`). Keep your mounted fixture directory aligned with those required
manifest paths; deferred Milestone 4 entries must not be treated as required CI blockers.

To list required Milestone 4 paths directly from the manifest:

```bash
awk '
  $1 == "[[roms]]" { in_block=1; milestone=0; required="false"; path="" }
  in_block && $1 == "milestone" { gsub(/_/, "", $3); milestone=$3 }
  in_block && $1 == "required" { required=$3 }
  in_block && $1 == "path" { path=$3; gsub(/"/, "", path) }
  in_block && path != "" && milestone == 4 && required == "true" { print path; in_block=0 }
' tests/rom_manifest.toml
```

### Manifest examples

Minimal example:

```toml
[[roms]]
id = "example-cpu-case"
suite = "blargg_cpu_instrs"
path = "blargg/cpu_instrs/individual/01-special.gb"
milestone = 2
required = true
cycle_limit = 20_000_000
frame_limit = 300
wall_time_limit_ms = 8_000
pass_condition = "blargg_mem"
```

Mooneye acceptance example:

```toml
[[roms]]
id = "example-mooneye-case"
suite = "mooneye_acceptance_cpu"
path = "mooneye/acceptance/add_sp_e_timing.gb"
milestone = 4
required = false
cycle_limit = 10_000_000
frame_limit = 180
wall_time_limit_ms = 8_000
pass_condition = "mooneye_registers"
```

## Running required Milestone 4 external validation locally

1. Ensure fixture files exist in the required layout under a local root directory.
2. Point `LATCHBOY_ROM_ROOT` at that directory.
3. Run the external validation test target:

```bash
LATCHBOY_ROM_ROOT=/absolute/path/to/rom-fixtures cargo test -p latchboy-core --test external_rom_validation
```

Optional: run only manifest structure checks (no ROM fixtures required):

```bash
cargo test -p latchboy-core --test external_rom_validation rom_manifest_registers_required_milestone_2_3_and_4_suites
```

Optional: run only required ROM execution/determinism gates (fixtures required):

```bash
LATCHBOY_ROM_ROOT=/absolute/path/to/rom-fixtures cargo test -p latchboy-core --test external_rom_validation required_milestone_2_3_and_4_roms_pass_under_external_validation_flow
LATCHBOY_ROM_ROOT=/absolute/path/to/rom-fixtures cargo test -p latchboy-core --test external_rom_validation required_milestone_2_3_and_4_rom_runs_are_deterministic
```

## CI gate for Milestone 2 completion

Milestone 2 is considered CI-complete when the GitHub Actions check run named:

- `CI / rust-checks`

is green on the target commit/PR. This maps to workflow `.github/workflows/ci.yml`, job key `rust-checks`, including the `Run tests` step (`cargo test --workspace --all-targets`) that executes `external_rom_validation` when `LATCHBOY_ROM_ROOT` is configured in CI.

## CI fixture provisioning for `LATCHBOY_ROM_ROOT` (contributors)

To avoid false-green runs where `external_rom_validation` is skipped, CI must provide a non-empty
`LATCHBOY_ROM_ROOT` that resolves every required manifest `path`.

The current workflow reads this value from a GitHub Actions repository variable:

- Workflow location: `.github/workflows/ci.yml`
- Job: `rust-checks`
- Environment mapping: `LATCHBOY_ROM_ROOT: ${{ vars.LATCHBOY_ROM_ROOT }}`

Recommended provisioning pattern for maintainers:

1. Build or mount a fixture directory in CI that matches `tests/rom_manifest.toml`.
2. Set repository variable **`LATCHBOY_ROM_ROOT`** to that absolute CI path (for example, `/opt/latchboy-roms`).
3. Ensure the configured path exists on the runner before the `Run tests` step.
4. Keep fixture contents synchronized with required manifest entries whenever required ROM cases are added or paths change.

Practical verification in CI logs:

- Confirm `cargo test --workspace --all-targets` runs `external_rom_validation`.
- Confirm there is no skip message indicating `LATCHBOY_ROM_ROOT` is unset/empty.
- Confirm required Milestone 2/3/4 cases execute and report pass within configured budgets.

## CI commands for required Milestone 4 suites

The default CI test command remains:

```bash
cargo test --workspace --all-targets
```

To mirror CI locally with explicit fixture wiring:

```bash
export LATCHBOY_ROM_ROOT=/absolute/path/to/rom-fixtures
cargo test --workspace --all-targets
```

To validate only the external ROM gate in CI-like mode:

```bash
LATCHBOY_ROM_ROOT=/absolute/path/to/rom-fixtures cargo test -p latchboy-core --test external_rom_validation
```

In GitHub Actions, `rust-checks` maps `LATCHBOY_ROM_ROOT` from `vars.LATCHBOY_ROM_ROOT`; this variable must resolve required manifest paths for the external validation run.

## Milestone 2 acceptance checklist → jobs/artifacts

- [ ] **Backlog bullet: “Passes CPU instruction correctness test ROMs.”**  
      Evidence: `CI / rust-checks` passes, and `cargo test --workspace --all-targets` includes successful `latchboy-core` external ROM validation with required Milestone 2 Blargg cases.
- [ ] **Backlog bullet: “Passes interrupt behavior test subset.”**  
      Evidence: `CI / rust-checks` passes with the workspace test run that includes CPU interrupt-focused tests in `latchboy-core` plus required Milestone 2 ROM cases in external validation.
- [ ] **Artifact check: Manifest and fixture contract is satisfied.**  
      Evidence: `tests/rom_manifest.toml` contains required `milestone = 2` + `required = true` entries; local/CI fixture tree resolves every required `path`.

## Milestone 4 completion checklist (blocking)

- [ ] **Milestone 4 remains blocked until required manifest entries pass.**  
      Blocker rule: if any `tests/rom_manifest.toml` entry with `milestone = 4` is marked `required = true`, all such entries must pass in `required_milestone_2_3_and_4_roms_pass_under_external_validation_flow` with `LATCHBOY_ROM_ROOT` set.
- [ ] **Determinism gate for required Milestone 4 entries passes.**  
      Evidence: `required_milestone_2_3_and_4_rom_runs_are_deterministic` is green under the same fixture root.
- [ ] **CI fixture wiring is present for external ROM validation.**  
      Evidence: `.github/workflows/ci.yml` job `rust-checks` exports `LATCHBOY_ROM_ROOT` and the configured directory contains all required Milestone 4 ROM fixture paths.
