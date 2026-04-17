use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use latchboy_core::cartridge::Cartridge;
use latchboy_core::Emulator;
use latchboy_desktop::savefile::{
    load_save_data_if_available, persist_save_data, save_path_from_rom_path,
    should_persist_after_load,
};

struct SaveOnDrop {
    cartridge: Cartridge,
    save_path: PathBuf,
    persist_enabled: bool,
}

impl Drop for SaveOnDrop {
    fn drop(&mut self) {
        if self.persist_enabled {
            persist_save_data(&self.cartridge, &self.save_path);
        }
    }
}

fn main() -> ExitCode {
    let rom_path = match env::args().nth(1) {
        Some(path) => PathBuf::from(path),
        None => {
            eprintln!("usage: latchboy-desktop <path-to-rom.gb>");
            return ExitCode::FAILURE;
        }
    };

    let rom_data = match fs::read(&rom_path) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!(
                "error: failed to read ROM '{}': {error}",
                rom_path.display()
            );
            return ExitCode::FAILURE;
        }
    };

    let mut cartridge = match Cartridge::from_rom(rom_data) {
        Ok(cartridge) => cartridge,
        Err(error) => {
            eprintln!(
                "error: failed to parse cartridge from ROM '{}': {error:?}",
                rom_path.display()
            );
            return ExitCode::FAILURE;
        }
    };

    let save_path = save_path_from_rom_path(&rom_path);
    let load_status = load_save_data_if_available(&mut cartridge, &save_path);
    let persist_enabled = should_persist_after_load(load_status);

    let _runtime = SaveOnDrop {
        cartridge: cartridge.clone(),
        save_path,
        persist_enabled,
    };

    let mut emulator = Emulator::from_cartridge(cartridge);
    let mut frame_presented = false;
    let max_cycles = 456_u32 * 154 * 2;
    let mut stepped = 0_u32;

    while stepped < max_cycles {
        let step = 4_096_u32.min(max_cycles - stepped);
        emulator.step_cycles(step);
        stepped += step;

        if emulator.take_frame_ready() {
            let mut shade_counts = [0u32; 4];
            for y in 0..144 {
                for x in 0..160 {
                    let shade = emulator.composited_pixel_shade(x, y) as usize;
                    shade_counts[shade] += 1;
                }
            }

            println!(
                "Latchboy desktop frontend scaffold: presented frame (shade histogram {:?})",
                shade_counts
            );
            frame_presented = true;
            break;
        }
    }

    if !frame_presented {
        eprintln!("warning: no frame-ready signal observed within {max_cycles} cycles");
    }

    ExitCode::SUCCESS
}
