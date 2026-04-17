use std::env;
use std::fs;
use std::io::{self, BufRead};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;

use latchboy_core::cartridge::Cartridge;
use latchboy_core::{Emulator, FRAMEBUFFER_HEIGHT, FRAMEBUFFER_WIDTH};
use latchboy_desktop::savefile::{
    load_save_data_if_available, persist_save_bytes, save_path_from_rom_path,
    should_persist_after_load,
};
use latchboy_desktop::{present_latest_frame, WindowSurface, DMG_PALETTE_ARGB8888};

const STEP_CYCLES: u32 = 4;

struct SaveOnDrop {
    emulator: Emulator,
    save_path: PathBuf,
    persist_enabled: bool,
}

impl Drop for SaveOnDrop {
    fn drop(&mut self) {
        if self.persist_enabled {
            if let Some(save_data) = self.emulator.save_data() {
                persist_save_bytes(&save_data, &self.save_path);
            }
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

    let emulator = Emulator::from_cartridge(cartridge);
    let mut runtime = SaveOnDrop {
        emulator,
        save_path,
        persist_enabled,
    };
    let quit_rx = spawn_quit_listener();

    let mut surface = WindowSurface::new(FRAMEBUFFER_WIDTH, FRAMEBUFFER_HEIGHT);
    loop {
        match quit_rx.try_recv() {
            Ok(()) | Err(TryRecvError::Disconnected) => break,
            Err(TryRecvError::Empty) => {}
        }
        runtime.emulator.step_cycles(STEP_CYCLES);
        if runtime.emulator.take_frame_ready() {
            if let Err(error) =
                present_latest_frame(&runtime.emulator, &mut surface, DMG_PALETTE_ARGB8888)
            {
                eprintln!("error: failed to present frame: {error}");
                return ExitCode::FAILURE;
            }
        }
    }

    ExitCode::SUCCESS
}

fn spawn_quit_listener() -> Receiver<()> {
    let (quit_tx, quit_rx) = mpsc::channel();
    thread::spawn(move || {
        let stdin = io::stdin();
        for line in stdin.lock().lines() {
            match line {
                Ok(command)
                    if command.eq_ignore_ascii_case("q")
                        || command.eq_ignore_ascii_case("quit") =>
                {
                    let _ = quit_tx.send(());
                    return;
                }
                Ok(_) => {}
                Err(_) => return,
            }
        }
    });
    quit_rx
}
