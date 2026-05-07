#[cfg(not(test))]
use criterion::{criterion_group, criterion_main};

#[cfg(not(test))]
mod bench_impl {
    use criterion::{BatchSize, Criterion};
    use latchboy_core::ppu::{
        Ppu, BGP_REGISTER, LCDC_REGISTER, OBP0_REGISTER, OBP1_REGISTER, SCX_REGISTER, SCY_REGISTER,
        WX_REGISTER, WY_REGISTER,
    };

    const DOTS_PER_FRAME: usize = 154 * 456;

    fn patterned_tile_row(tile_id: u8, row: u8) -> (u8, u8) {
        let low = tile_id.rotate_left(u32::from(row & 0x07)) ^ (0x11u8.wrapping_mul(row + 1));
        let high = !tile_id.rotate_right(u32::from(row & 0x07)) ^ (0x80 >> (row & 0x07));
        (low, high)
    }

    fn build_busy_ppu_scene() -> Ppu {
        let mut ppu = Ppu::default();

        for tile_id in 0u16..=255 {
            for row in 0u16..8 {
                let (low, high) = patterned_tile_row(tile_id as u8, row as u8);
                let row_address = 0x8000 + tile_id * 16 + row * 2;
                ppu.write_vram(row_address, low);
                ppu.write_vram(row_address + 1, high);
            }
        }

        for tile_index in 0u16..1024 {
            let bg_tile = tile_index.wrapping_mul(13).wrapping_add(tile_index / 32) as u8;
            let window_tile = tile_index.wrapping_mul(7).wrapping_add(tile_index % 32) as u8;
            ppu.write_vram(0x9800 + tile_index, bg_tile);
            ppu.write_vram(0x9C00 + tile_index, window_tile);
        }

        for sprite_index in 0u16..40 {
            let base = 0xFE00 + sprite_index * 4;
            let y = 16 + ((sprite_index * 7) % 144) as u8;
            let x = 8 + ((sprite_index * 11) % 160) as u8;
            let attributes = match sprite_index % 4 {
                0 => 0x00,
                1 => 0x20,
                2 => 0x40,
                _ => 0x10,
            };

            ppu.write_oam(base, y);
            ppu.write_oam(base + 1, x);
            ppu.write_oam(base + 2, sprite_index.wrapping_mul(3) as u8);
            ppu.write_oam(base + 3, attributes);
        }

        ppu.write_register(SCY_REGISTER, 9);
        ppu.write_register(SCX_REGISTER, 13);
        ppu.write_register(BGP_REGISTER, 0b11_10_01_00);
        ppu.write_register(OBP0_REGISTER, 0b00_01_10_11);
        ppu.write_register(OBP1_REGISTER, 0b01_11_00_10);
        ppu.write_register(WY_REGISTER, 48);
        ppu.write_register(WX_REGISTER, 24);
        ppu.write_register(LCDC_REGISTER, 0x80 | 0x20 | 0x10 | 0x02 | 0x01);

        ppu
    }

    fn step_one_frame(mut ppu: Ppu) {
        let mut interrupt_flag = 0;
        for _ in 0..DOTS_PER_FRAME {
            ppu.step(&mut interrupt_flag);
        }
        criterion::black_box((interrupt_flag, ppu.framebuffer_pixels()[0]));
    }

    pub fn bench_ppu_scanline_throughput(c: &mut Criterion) {
        let scene = build_busy_ppu_scene();

        c.bench_function("ppu_scanline_throughput_busy_frame", |b| {
            b.iter_batched(|| scene.clone(), step_one_frame, BatchSize::SmallInput)
        });
    }
}

#[cfg(not(test))]
criterion_group!(benches, bench_impl::bench_ppu_scanline_throughput);
#[cfg(not(test))]
criterion_main!(benches);

#[cfg(test)]
fn main() {}
