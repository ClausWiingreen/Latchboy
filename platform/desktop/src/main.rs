use std::env;
use std::fs;
use std::path::PathBuf;

use latchboy_core::cartridge::Cartridge;
use latchboy_desktop::savefile::{
    load_save_data_if_available, persist_save_data, save_path_from_rom_path,
};

struct SaveOnDrop {
    cartridge: Cartridge,
    save_path: PathBuf,
}

impl Drop for SaveOnDrop {
    fn drop(&mut self) {
        persist_save_data(&self.cartridge, &self.save_path);
    }
}

fn main() {
    let rom_path = match env::args().nth(1) {
        Some(path) => PathBuf::from(path),
        None => {
            eprintln!("usage: latchboy-desktop <path-to-rom.gb>");
            return;
        }
    };

    let rom_data = match fs::read(&rom_path) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!(
                "error: failed to read ROM '{}': {error}",
                rom_path.display()
            );
            return;
        }
    };

    let mut cartridge = match Cartridge::from_rom(rom_data) {
        Ok(cartridge) => cartridge,
        Err(error) => {
            eprintln!(
                "error: failed to parse cartridge from ROM '{}': {error:?}",
                rom_path.display()
            );
            return;
        }
    };

    let save_path = save_path_from_rom_path(&rom_path);
    load_save_data_if_available(&mut cartridge, &save_path);

    let _runtime = SaveOnDrop {
        cartridge,
        save_path,
    };

    println!("Latchboy desktop frontend scaffold");
}
