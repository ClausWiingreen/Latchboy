use criterion::{criterion_group, criterion_main, BatchSize, Criterion};

use latchboy_core::FRAMEBUFFER_LEN;
use latchboy_desktop::blit_dmg_framebuffer_to_rgb_surface;

fn bench_framebuffer_blit(c: &mut Criterion) {
    c.bench_function("blit_dmg_framebuffer_to_rgb_surface_full_frame", |b| {
        b.iter_batched(
            || {
                let framebuffer: Vec<u8> = (0..FRAMEBUFFER_LEN)
                    .map(|index| (index % 4) as u8)
                    .collect();
                let surface = vec![0u32; FRAMEBUFFER_LEN];
                (framebuffer, surface)
            },
            |(framebuffer, mut surface)| {
                blit_dmg_framebuffer_to_rgb_surface(&framebuffer, &mut surface)
                    .expect("benchmark framebuffer sizes should match");
            },
            BatchSize::SmallInput,
        )
    });
}

criterion_group!(benches, bench_framebuffer_blit);
criterion_main!(benches);
