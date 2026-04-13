# Emulator Validation Tests

This directory is reserved for ROM-based integration validation, deterministic headless harnesses, and golden output fixtures.

## Current deterministic harness

A deterministic headless harness scaffold lives in the `latchboy-core` test suite and can be executed with:

```bash
cargo test -p latchboy-core --test headless_harness
```

The harness currently validates that cycle stepping and reset behavior are reproducible,
using a test-side hash of observable emulator state (`total_cycles`) for deterministic assertions.
