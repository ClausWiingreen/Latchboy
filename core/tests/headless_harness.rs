use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use latchboy_core::Emulator;

fn emulator_hash(emulator: &Emulator) -> u64 {
    let mut hasher = DefaultHasher::new();
    emulator.hash(&mut hasher);
    hasher.finish()
}

#[test]
fn headless_harness_is_deterministic_for_equal_cycle_totals() {
    let mut run_a = Emulator::new();
    run_a.step_cycles(512);
    run_a.step_cycles(1024);

    let mut run_b = Emulator::new();
    run_b.step_cycles(1536);

    assert_eq!(run_a.total_cycles(), run_b.total_cycles());
    assert_eq!(emulator_hash(&run_a), emulator_hash(&run_b));
}

#[test]
fn headless_harness_reset_restores_initial_state() {
    let mut emulator = Emulator::new();
    emulator.step_cycles(4096);
    assert_ne!(emulator.total_cycles(), 0);

    emulator.reset();

    assert_eq!(emulator.total_cycles(), 0);
    assert_eq!(emulator_hash(&emulator), emulator_hash(&Emulator::new()));
}
