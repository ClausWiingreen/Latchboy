mod common;

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};

use common::cartridge_with_program;
use latchboy_core::Emulator;

fn bench_interrupt_heavy_stepping(c: &mut Criterion) {
    c.bench_function("interrupt_heavy_timer_wakeup", |b| {
        b.iter_batched(
            || {
                Emulator::from_cartridge(cartridge_with_program(
                    &[
                        (0x0050, 0x3C), // Timer ISR: INC A
                        (0x0051, 0xD9), // RETI
                        (0x0100, 0xF3), // DI
                        (0x0101, 0x3E), // LD A, d8
                        (0x0102, 0x00),
                        (0x0103, 0xE0), // LDH (FF06), A ; TMA
                        (0x0104, 0x06),
                        (0x0105, 0x3E), // LD A, d8
                        (0x0106, 0xFC),
                        (0x0107, 0xE0), // LDH (FF05), A ; TIMA near overflow
                        (0x0108, 0x05),
                        (0x0109, 0x3E), // LD A, d8
                        (0x010A, 0x05),
                        (0x010B, 0xE0), // LDH (FF07), A ; TAC enable + fast clock
                        (0x010C, 0x07),
                        (0x010D, 0x3E), // LD A, d8
                        (0x010E, 0x04),
                        (0x010F, 0xE0), // LDH (FFFF), A ; IE timer bit
                        (0x0110, 0xFF),
                        (0x0111, 0xFB), // EI
                        (0x0112, 0x76), // HALT
                        (0x0113, 0x18), // JR r8
                        (0x0114, 0xFD), // back to HALT
                    ],
                    b"INTR",
                ))
            },
            |mut emulator| {
                emulator.step_cycles(70_224);
            },
            BatchSize::SmallInput,
        )
    });
}

criterion_group!(benches, bench_interrupt_heavy_stepping);
criterion_main!(benches);
