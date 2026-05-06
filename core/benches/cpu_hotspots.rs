mod common;

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};

use common::cartridge_with_program;
use latchboy_core::Emulator;

fn bench_register_alu_dispatch(c: &mut Criterion) {
    let cartridge = cartridge_with_program(
        &[
            (0x0100, 0x3E), // LD A, d8
            (0x0101, 0x5A),
            (0x0102, 0x06), // LD B, d8
            (0x0103, 0x21),
            (0x0104, 0x80), // ADD A, B
            (0x0105, 0x88), // ADC A, B
            (0x0106, 0x90), // SUB B
            (0x0107, 0x98), // SBC A, B
            (0x0108, 0xA0), // AND B
            (0x0109, 0xA8), // XOR B
            (0x010A, 0xB0), // OR B
            (0x010B, 0xB8), // CP B
            (0x010C, 0x18), // JR r8
            (0x010D, 0xF5), // back to ALU mix at 0x0104
        ],
        b"ALUH",
    );

    c.bench_function("cpu_hotspot_register_alu_dispatch", |b| {
        b.iter_batched(
            || Emulator::from_cartridge(cartridge.clone()),
            |mut emulator| emulator.step_cycles(70_224),
            BatchSize::SmallInput,
        )
    });
}

fn bench_cb_prefixed_bit_ops(c: &mut Criterion) {
    let cartridge = cartridge_with_program(
        &[
            (0x0100, 0x21), // LD HL, d16
            (0x0101, 0x00),
            (0x0102, 0xC0),
            (0x0103, 0x36), // LD (HL), d8
            (0x0104, 0x81),
            (0x0105, 0x0E), // LD C, d8
            (0x0106, 0x3C),
            (0x0107, 0xCB), // BIT 7, (HL)
            (0x0108, 0x7E),
            (0x0109, 0xCB), // SET 0, (HL)
            (0x010A, 0xC6),
            (0x010B, 0xCB), // RES 0, (HL)
            (0x010C, 0x86),
            (0x010D, 0xCB), // RL C
            (0x010E, 0x11),
            (0x010F, 0xCB), // SWAP C
            (0x0110, 0x31),
            (0x0111, 0xCB), // SRL C
            (0x0112, 0x39),
            (0x0113, 0x18), // JR r8
            (0x0114, 0xF2), // back to CB mix at 0x0107
        ],
        b"CBHS",
    );

    c.bench_function("cpu_hotspot_cb_prefixed_bit_ops", |b| {
        b.iter_batched(
            || Emulator::from_cartridge(cartridge.clone()),
            |mut emulator| emulator.step_cycles(70_224),
            BatchSize::SmallInput,
        )
    });
}

fn bench_stack_and_control_flow(c: &mut Criterion) {
    let cartridge = cartridge_with_program(
        &[
            (0x0100, 0x31), // LD SP, d16
            (0x0101, 0xFE),
            (0x0102, 0xFF),
            (0x0103, 0x01), // LD BC, d16
            (0x0104, 0x34),
            (0x0105, 0x12),
            (0x0106, 0x11), // LD DE, d16
            (0x0107, 0x78),
            (0x0108, 0x56),
            (0x0109, 0xCD), // CALL 0x0120
            (0x010A, 0x20),
            (0x010B, 0x01),
            (0x010C, 0x18), // JR r8
            (0x010D, 0xFA), // back to CALL at 0x0109
            (0x0120, 0xC5), // PUSH BC
            (0x0121, 0xD5), // PUSH DE
            (0x0122, 0xD1), // POP DE
            (0x0123, 0xC1), // POP BC
            (0x0124, 0xC9), // RET
        ],
        b"STCK",
    );

    c.bench_function("cpu_hotspot_stack_and_control_flow", |b| {
        b.iter_batched(
            || Emulator::from_cartridge(cartridge.clone()),
            |mut emulator| emulator.step_cycles(70_224),
            BatchSize::SmallInput,
        )
    });
}

criterion_group!(
    benches,
    bench_register_alu_dispatch,
    bench_cb_prefixed_bit_ops,
    bench_stack_and_control_flow
);
criterion_main!(benches);
