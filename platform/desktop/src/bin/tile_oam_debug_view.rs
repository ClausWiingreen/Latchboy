use std::fs;
use std::path::PathBuf;

use clap::Parser;
use latchboy_core::{cartridge::Cartridge, Emulator};
use latchboy_desktop::{write_rgb_surface_to_png, DMG_PALETTE_RGB};

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

fn main() -> Result<(), String> {
    let args = Args::parse();
    let rom_bytes = fs::read(&args.rom_path).map_err(|error| {
        format!(
            "failed to read ROM '{}': {error:?}",
            args.rom_path.display()
        )
    })?;
    let cartridge = Cartridge::from_rom(rom_bytes).map_err(|error| {
        format!(
            "failed to parse ROM '{}': {error:?}",
            args.rom_path.display()
        )
    })?;

    let mut emulator = Emulator::from_cartridge(cartridge);
    let mut frames_seen = 0u64;
    while frames_seen < args.frames {
        emulator.step_cycles(1_024);
        if emulator.take_frame_ready() {
            frames_seen += 1;
        }
    }

    fs::create_dir_all(&args.output_dir).map_err(|error| {
        format!(
            "failed to create output dir '{}': {error}",
            args.output_dir.display()
        )
    })?;

    let atlas = build_tile_atlas(&emulator);
    let atlas_path = args.output_dir.join("tile-atlas.png");
    write_rgb_surface_to_png(
        &atlas_path,
        &atlas,
        (ATLAS_COLUMNS * TILE_SIZE) as u32,
        (ATLAS_ROWS * TILE_SIZE) as u32,
    )
    .map_err(|error| format!("failed to write '{}': {error}", atlas_path.display()))?;

    let oam_path = args.output_dir.join("oam.txt");
    fs::write(&oam_path, build_oam_dump(&emulator))
        .map_err(|error| format!("failed to write '{}': {error}", oam_path.display()))?;

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
