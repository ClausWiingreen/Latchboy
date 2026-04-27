# Benchmarking Guide

This repository uses [Criterion](https://bheisler.github.io/criterion.rs/book/index.html) benchmarks for stable, statistically meaningful performance tracking.

## Run benchmarks

- Core emulator benchmarks:
  - `cargo bench -p latchboy-core`
- Desktop blit benchmark:
  - `cargo bench -p latchboy-desktop`

Tip: for quick local iteration, run a single target:

- `cargo bench -p latchboy-core --bench emulator_step_cycles`
- `cargo bench -p latchboy-core --bench instruction_stepping`
- `cargo bench -p latchboy-core --bench interrupt_heavy`
- `cargo bench -p latchboy-desktop --bench framebuffer_blit`

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
