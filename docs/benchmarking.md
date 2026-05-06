# Benchmarking Guide

This repository uses [Criterion](https://bheisler.github.io/criterion.rs/book/index.html) benchmarks for stable, statistically meaningful performance tracking.

## Run benchmarks

- Core emulator benchmarks:
  - `cargo bench -p latchboy-core`
- Desktop blit benchmark:
  - `cargo bench -p latchboy-desktop`

Tip: for quick local iteration, run a single target:

- `cargo bench -p latchboy-core --bench audio_callback_stability`
- `cargo bench -p latchboy-core --bench cpu_hotspots`
- `cargo bench -p latchboy-core --bench emulator_step_cycles`
- `cargo bench -p latchboy-core --bench instruction_stepping`
- `cargo bench -p latchboy-core --bench interrupt_heavy`
- `cargo bench -p latchboy-core --bench ppu_scanline_throughput`
- `cargo bench -p latchboy-desktop --bench framebuffer_blit`

## Audio callback stability coverage

Use `audio_callback_stability` as the focused APU output queue and audio-device callback profiling entry point. It simulates a steady 512-sample callback cadence while the APU produces mixed output for all tone/wave channels:

- `audio_callback_stability_512_sample_cadence`: prefilled queue trimming plus producer ticks and fixed-size callback pulls over 120 callbacks.

## CPU hotspot coverage

Use `cpu_hotspots` as the focused CPU-dispatch profiling entry point. It isolates three instruction families that are easy to regress while tuning the emulator loop:

- `cpu_hotspot_register_alu_dispatch`: tight register ALU opcode dispatch.
- `cpu_hotspot_cb_prefixed_bit_ops`: CB-prefixed register and `(HL)` bit operations.
- `cpu_hotspot_stack_and_control_flow`: CALL/RET plus PUSH/POP-heavy control flow.

## PPU scanline coverage

Use `ppu_scanline_throughput` as the focused PPU rendering throughput entry point. It steps one busy DMG frame through the PPU dot pipeline with background, window, and sprite composition enabled:

- `ppu_scanline_throughput_busy_frame`: visible scanline stepping and framebuffer composition for a populated scene.

## PR review baseline metrics

When a PR changes emulation timing, CPU execution, PPU/framebuffer behavior, or desktop blitting, compare the following metrics against the current `main` baseline from Criterion output:

- **Median time (`time` estimate)** for each benchmark target.
- **Relative change (%)** vs baseline run.
- **Noise threshold check** (treat changes within ±5% as likely noise unless repeated).

Suggested review rubric:

- **Green:** all benchmark medians within ±5%.
- **Investigate:** any benchmark regresses by >5%.
- **Action required:** any benchmark regresses by >10% without a documented reason.

If a larger slowdown is intentional (e.g., correctness fix), call it out in the PR description with expected impact and follow-up optimization notes.
