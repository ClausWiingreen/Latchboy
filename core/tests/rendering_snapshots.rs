use std::{collections::BTreeMap, fs, path::Path};

use latchboy_core::ppu::{
    Ppu, BGP_REGISTER, FRAMEBUFFER_LEN, LCDC_REGISTER, OBP0_REGISTER, OBP1_REGISTER, SCX_REGISTER,
    SCY_REGISTER, WX_REGISTER, WY_REGISTER,
};
use serde::Deserialize;

const SNAPSHOT_PATH: &str = "../tests/rendering_snapshots.toml";
const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x100000001b3;

#[derive(Debug, Deserialize)]
struct SnapshotFile {
    snapshots: BTreeMap<String, Snapshot>,
}

#[derive(Debug, Deserialize)]
struct Snapshot {
    framebuffer_fnv1a64: String,
}

fn framebuffer_hash(framebuffer: &[u8]) -> String {
    let mut hash = FNV_OFFSET_BASIS;
    for pixel in framebuffer {
        hash ^= u64::from(*pixel);
        hash = hash.wrapping_mul(FNV_PRIME);
    }

    format!("{hash:016x}")
}

fn write_tile(ppu: &mut Ppu, tile_index: u8, rows: [[u8; 8]; 8]) {
    let base = 0x8000 + u16::from(tile_index) * 16;
    for (row, colors) in rows.into_iter().enumerate() {
        let mut low = 0u8;
        let mut high = 0u8;
        for (x, color) in colors.into_iter().enumerate() {
            let bit = 7 - x;
            low |= (color & 0x01) << bit;
            high |= ((color >> 1) & 0x01) << bit;
        }
        ppu.write_vram(base + row as u16 * 2, low);
        ppu.write_vram(base + row as u16 * 2 + 1, high);
    }
}

fn write_bg_map(ppu: &mut Ppu, map_base: u16, tile_for_cell: impl Fn(usize, usize) -> u8) {
    for y in 0..32 {
        for x in 0..32 {
            ppu.write_vram(map_base + (y * 32 + x) as u16, tile_for_cell(x, y));
        }
    }
}

fn render_until_frame_ready(ppu: &mut Ppu) -> [u8; FRAMEBUFFER_LEN] {
    let mut interrupt_flag = 0u8;
    for _ in 0..(456 * 154 + 32) {
        ppu.step(&mut interrupt_flag);
        if ppu.take_frame_ready() {
            return *ppu.framebuffer();
        }
    }

    panic!("PPU did not produce a frame-ready snapshot within one frame budget");
}

fn scrolled_checkerboard_background() -> [u8; FRAMEBUFFER_LEN] {
    let mut ppu = Ppu::default();
    write_tile(
        &mut ppu,
        0,
        [
            [0, 1, 2, 3, 0, 1, 2, 3],
            [1, 2, 3, 0, 1, 2, 3, 0],
            [2, 3, 0, 1, 2, 3, 0, 1],
            [3, 0, 1, 2, 3, 0, 1, 2],
            [0, 1, 2, 3, 0, 1, 2, 3],
            [1, 2, 3, 0, 1, 2, 3, 0],
            [2, 3, 0, 1, 2, 3, 0, 1],
            [3, 0, 1, 2, 3, 0, 1, 2],
        ],
    );
    write_tile(
        &mut ppu,
        1,
        [
            [3, 2, 1, 0, 3, 2, 1, 0],
            [2, 1, 0, 3, 2, 1, 0, 3],
            [1, 0, 3, 2, 1, 0, 3, 2],
            [0, 3, 2, 1, 0, 3, 2, 1],
            [3, 2, 1, 0, 3, 2, 1, 0],
            [2, 1, 0, 3, 2, 1, 0, 3],
            [1, 0, 3, 2, 1, 0, 3, 2],
            [0, 3, 2, 1, 0, 3, 2, 1],
        ],
    );
    write_bg_map(&mut ppu, 0x9800, |x, y| ((x + y) % 2) as u8);
    ppu.write_register(BGP_REGISTER, 0xE4);
    ppu.write_register(SCX_REGISTER, 3);
    ppu.write_register(SCY_REGISTER, 5);
    ppu.write_register(LCDC_REGISTER, 0x91);
    render_until_frame_ready(&mut ppu)
}

fn window_sprite_priority_scene() -> [u8; FRAMEBUFFER_LEN] {
    let mut ppu = Ppu::default();
    write_tile(&mut ppu, 0, [[0; 8]; 8]);
    write_tile(&mut ppu, 1, [[1; 8]; 8]);
    write_tile(&mut ppu, 2, [[2; 8]; 8]);
    write_tile(
        &mut ppu,
        3,
        [
            [3, 0, 3, 0, 3, 0, 3, 0],
            [0, 3, 0, 3, 0, 3, 0, 3],
            [3, 0, 3, 0, 3, 0, 3, 0],
            [0, 3, 0, 3, 0, 3, 0, 3],
            [3, 0, 3, 0, 3, 0, 3, 0],
            [0, 3, 0, 3, 0, 3, 0, 3],
            [3, 0, 3, 0, 3, 0, 3, 0],
            [0, 3, 0, 3, 0, 3, 0, 3],
        ],
    );
    write_bg_map(&mut ppu, 0x9800, |x, y| {
        if (x / 2 + y / 2) % 2 == 0 {
            1
        } else {
            0
        }
    });
    write_bg_map(
        &mut ppu,
        0x9C00,
        |x, y| if (x + y) % 3 == 0 { 2 } else { 0 },
    );

    ppu.write_oam(0xFE00, 48); // screen y = 32
    ppu.write_oam(0xFE01, 40); // screen x = 32
    ppu.write_oam(0xFE02, 3);
    ppu.write_oam(0xFE03, 0x00);
    ppu.write_oam(0xFE04, 52); // overlaps, lower X wins after sorting
    ppu.write_oam(0xFE05, 38);
    ppu.write_oam(0xFE06, 3);
    ppu.write_oam(0xFE07, 0x90); // behind non-zero BG + OBP1

    ppu.write_register(BGP_REGISTER, 0xE4);
    ppu.write_register(OBP0_REGISTER, 0xD2);
    ppu.write_register(OBP1_REGISTER, 0x24);
    ppu.write_register(WY_REGISTER, 24);
    ppu.write_register(WX_REGISTER, 47);
    ppu.write_register(LCDC_REGISTER, 0xF3);
    render_until_frame_ready(&mut ppu)
}

fn load_snapshots() -> SnapshotFile {
    let manifest_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(SNAPSHOT_PATH);
    let manifest =
        fs::read_to_string(&manifest_path).expect("snapshot manifest should be readable");
    toml::from_str(&manifest).expect("snapshot manifest should parse")
}

#[test]
fn ppu_rendering_snapshots_match_golden_hashes() {
    let snapshots = load_snapshots();
    let cases = [
        (
            "scrolled_checkerboard_background",
            scrolled_checkerboard_background(),
        ),
        (
            "window_sprite_priority_scene",
            window_sprite_priority_scene(),
        ),
    ];

    for (case_name, framebuffer) in cases {
        let expected = snapshots
            .snapshots
            .get(case_name)
            .unwrap_or_else(|| panic!("missing rendering snapshot case `{case_name}`"));
        assert_eq!(
            framebuffer_hash(&framebuffer),
            expected.framebuffer_fnv1a64,
            "rendering snapshot `{case_name}` changed; update {SNAPSHOT_PATH} only after reviewing the framebuffer diff"
        );
    }
}
