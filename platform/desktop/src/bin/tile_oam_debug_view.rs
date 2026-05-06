use std::path::PathBuf;

use clap::Parser;
use latchboy_core::Emulator;
use latchboy_desktop::{
    debug_harness::{
        ensure_output_dir, load_emulator, step_frames, write_rgb_surface_png, write_text_file,
        DebugHarnessResult,
    },
    DMG_PALETTE_RGB,
};

const TILE_SIZE: usize = 8;
const TILE_BYTES: usize = 16;
const TILE_COUNT: usize = 384;
const ATLAS_COLUMNS: usize = 16;
const ATLAS_ROWS: usize = TILE_COUNT / ATLAS_COLUMNS;

#[derive(Debug, Parser)]
#[command(name = "tile_oam_debug_view")]
struct Args {
    /// ROM path.
    rom_path: PathBuf,
    /// Output directory for generated artifacts.
    #[arg(long)]
    output_dir: PathBuf,
    /// Number of frames to run before capture.
    #[arg(long, default_value_t = 1)]
    frames: u64,
}

fn main() -> DebugHarnessResult<()> {
    let args = Args::parse();
    let mut emulator = load_emulator(&args.rom_path)?;
    step_frames(&mut emulator, args.frames, 1_024)?;

    ensure_output_dir(&args.output_dir)?;

    let atlas = build_tile_atlas(&emulator);
    let atlas_path = args.output_dir.join("tile-atlas.png");
    write_rgb_surface_png(
        &atlas_path,
        &atlas,
        (ATLAS_COLUMNS * TILE_SIZE) as u32,
        (ATLAS_ROWS * TILE_SIZE) as u32,
    )?;

    let oam_path = args.output_dir.join("oam.txt");
    write_text_file(&oam_path, build_oam_dump(&emulator))?;

    println!("wrote {} and {}", atlas_path.display(), oam_path.display());
    Ok(())
}

fn build_tile_atlas(emulator: &Emulator) -> Vec<u32> {
    let width = ATLAS_COLUMNS * TILE_SIZE;
    let height = ATLAS_ROWS * TILE_SIZE;
    let mut surface = vec![0u32; width * height];

    for tile_index in 0..TILE_COUNT {
        let tile_base = 0x8000 + (tile_index * TILE_BYTES) as u16;
        let tile_x = (tile_index % ATLAS_COLUMNS) * TILE_SIZE;
        let tile_y = (tile_index / ATLAS_COLUMNS) * TILE_SIZE;

        for row in 0..TILE_SIZE {
            let low = emulator.bus().read8(tile_base + (row * 2) as u16);
            let high = emulator.bus().read8(tile_base + (row * 2 + 1) as u16);
            for col in 0..TILE_SIZE {
                let shift = 7 - col;
                let lo = (low >> shift) & 0x01;
                let hi = (high >> shift) & 0x01;
                let shade = ((hi << 1) | lo) as usize;
                let dst_x = tile_x + col;
                let dst_y = tile_y + row;
                surface[dst_y * width + dst_x] = DMG_PALETTE_RGB[shade];
            }
        }
    }

    surface
}

fn build_oam_dump(emulator: &Emulator) -> String {
    let mut out = String::from("index,x,y,tile,flags\n");
    for sprite_index in 0..40u16 {
        let base = 0xFE00 + sprite_index * 4;
        let y = emulator.bus().read8(base);
        let x = emulator.bus().read8(base + 1);
        let tile = emulator.bus().read8(base + 2);
        let flags = emulator.bus().read8(base + 3);
        out.push_str(&format!(
            "{sprite_index},{x},{y},{tile:#04X},{flags:#04X}\n"
        ));
    }
    out
}
