mod common;

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};

use common::cartridge_with_program;
use latchboy_core::Emulator;

fn bench_emulator_step_cycles(c: &mut Criterion) {
    let cartridge = cartridge_with_program(
        &[
            (0x0100, 0x06), // LD B, d8
            (0x0101, 0x00),
            (0x0102, 0x04), // INC B
            (0x0103, 0x05), // DEC B
            (0x0104, 0x00), // NOP
            (0x0105, 0x18), // JR r8
            (0x0106, 0xFB), // back to 0x0102
        ],
        b"STEP",
    );

    c.bench_function("emulator_step_cycles_70224", |b| {
        b.iter_batched(
            || Emulator::from_cartridge(cartridge.clone()),
            |mut emulator| {
                emulator.step_cycles(70_224);
            },
            BatchSize::SmallInput,
        )
    });
}

criterion_group!(benches, bench_emulator_step_cycles);
criterion_main!(benches);
