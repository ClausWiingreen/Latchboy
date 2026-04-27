mod common;

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};

use common::cartridge_with_program;
use latchboy_core::Emulator;

fn bench_instruction_heavy_stepping(c: &mut Criterion) {
    c.bench_function("instruction_heavy_step_loops", |b| {
        b.iter_batched(
            || {
                Emulator::from_cartridge(cartridge_with_program(
                    &[
                        (0x0100, 0x3E), // LD A, d8
                        (0x0101, 0x10),
                        (0x0102, 0x06), // LD B, d8
                        (0x0103, 0x20),
                        (0x0104, 0x80), // ADD A, B
                        (0x0105, 0x90), // SUB B
                        (0x0106, 0x3C), // INC A
                        (0x0107, 0x05), // DEC B
                        (0x0108, 0x20), // JR NZ, r8
                        (0x0109, 0xFA), // back to 0x0104 while B != 0
                        (0x010A, 0x18), // JR r8
                        (0x010B, 0xF8), // back to 0x0104
                    ],
                    b"INST",
                ))
            },
            |mut emulator| {
                for _ in 0..8 {
                    emulator.step_cycles(8_192);
                }
            },
            BatchSize::SmallInput,
        )
    });
}

criterion_group!(benches, bench_instruction_heavy_stepping);
criterion_main!(benches);
