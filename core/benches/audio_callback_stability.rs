use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use latchboy_core::apu::Apu;

const CALLBACK_SAMPLES: usize = 512;
const CALLBACKS_PER_RUN: usize = 120;
const CALLBACK_T_CYCLES: u32 =
    (CALLBACK_SAMPLES as u32 * Apu::DMG_CLOCK_HZ) / Apu::OUTPUT_SAMPLE_RATE_HZ;
const PREFILL_T_CYCLES: u32 = Apu::DMG_CLOCK_HZ / 10;

fn configured_apu() -> Apu {
    let mut apu = Apu::new();
    apu.set_ch1_enabled(true);
    apu.set_ch2_enabled(true);
    apu.set_ch3_enabled(true);
    assert!(apu.write_register(0xFF24, 0x77));
    assert!(apu.write_register(0xFF25, 0x77));

    let _ = apu.tick(PREFILL_T_CYCLES);
    assert!(apu.queued_samples() > Apu::OUTPUT_QUEUE_TARGET_SAMPLES);

    apu
}

fn run_callback_cadence(mut apu: Apu) {
    let mut checksum = 0_i64;

    for _ in 0..CALLBACKS_PER_RUN {
        let _ = apu.tick(CALLBACK_T_CYCLES);
        let samples = apu.pull_output_samples(CALLBACK_SAMPLES);
        checksum += samples.iter().map(|sample| i64::from(*sample)).sum::<i64>();
    }

    criterion::black_box((checksum, apu.queued_samples()));
}

fn bench_audio_callback_stability(c: &mut Criterion) {
    c.bench_function("audio_callback_stability_512_sample_cadence", |b| {
        b.iter_batched(configured_apu, run_callback_cadence, BatchSize::SmallInput)
    });
}

criterion_group!(benches, bench_audio_callback_stability);
criterion_main!(benches);
