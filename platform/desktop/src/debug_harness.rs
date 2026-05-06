use std::fs;
use std::path::{Path, PathBuf};

use latchboy_core::{cartridge::Cartridge, Emulator};
use serde::Serialize;
use thiserror::Error;

use crate::{write_rgb_surface_to_png, DesktopRuntimeError, SurfaceImageWriteError};

#[derive(Debug, Error)]
pub enum DebugHarnessError {
    #[error("failed to read ROM '{path}': {source}")]
    ReadRom {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse cartridge from ROM '{path}': {reason}")]
    ParseCartridge { path: PathBuf, reason: String },
    #[error("debug harness runtime failed: {0}")]
    Runtime(#[from] DesktopRuntimeError),
    #[error("failed to create directory '{path}': {source}")]
    CreateDir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to write file '{path}': {source}")]
    WriteFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to serialize JSON for '{path}': {source}")]
    SerializeJson {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("failed to execute command `{command}`: {source}")]
    CommandIo {
        command: String,
        #[source]
        source: std::io::Error,
    },
    #[error("command `{command}` failed: {message}")]
    CommandFailed { command: String, message: String },
    #[error("failed to write PNG: {0}")]
    WritePng(#[from] SurfaceImageWriteError),
}

pub type DebugHarnessResult<T> = Result<T, DebugHarnessError>;

pub fn load_rom_file(path: impl AsRef<Path>) -> DebugHarnessResult<Vec<u8>> {
    let path = path.as_ref();
    fs::read(path).map_err(|source| DebugHarnessError::ReadRom {
        path: path.to_path_buf(),
        source,
    })
}

pub fn cartridge_from_rom(path: impl AsRef<Path>, rom: Vec<u8>) -> DebugHarnessResult<Cartridge> {
    let path = path.as_ref();
    Cartridge::from_rom(rom).map_err(|error| DebugHarnessError::ParseCartridge {
        path: path.to_path_buf(),
        reason: format!("{error:?}"),
    })
}

pub fn load_cartridge(path: impl AsRef<Path>) -> DebugHarnessResult<Cartridge> {
    let path = path.as_ref();
    let rom = load_rom_file(path)?;
    cartridge_from_rom(path, rom)
}

pub fn load_emulator(path: impl AsRef<Path>) -> DebugHarnessResult<Emulator> {
    load_cartridge(path).map(Emulator::from_cartridge)
}

pub fn step_until_frame_ready(emulator: &mut Emulator, cycle_step: u32) -> DebugHarnessResult<u64> {
    if cycle_step == 0 {
        return Err(DesktopRuntimeError::InvalidCycleStep.into());
    }

    let mut cycles = 0u64;
    while !emulator.take_frame_ready() {
        emulator.step_cycles(cycle_step);
        cycles = cycles.saturating_add(u64::from(cycle_step));
    }
    Ok(cycles)
}

pub fn step_frames(
    emulator: &mut Emulator,
    frames: u64,
    cycle_step: u32,
) -> DebugHarnessResult<u64> {
    let mut cycles = 0u64;
    for _ in 0..frames {
        cycles = cycles.saturating_add(step_until_frame_ready(emulator, cycle_step)?);
    }
    Ok(cycles)
}

pub fn ensure_output_dir(path: impl AsRef<Path>) -> DebugHarnessResult<()> {
    let path = path.as_ref();
    fs::create_dir_all(path).map_err(|source| DebugHarnessError::CreateDir {
        path: path.to_path_buf(),
        source,
    })
}

pub fn write_text_file(
    path: impl AsRef<Path>,
    contents: impl AsRef<[u8]>,
) -> DebugHarnessResult<()> {
    let path = path.as_ref();
    fs::write(path, contents).map_err(|source| DebugHarnessError::WriteFile {
        path: path.to_path_buf(),
        source,
    })
}

pub fn write_json_summary<T: Serialize>(
    path: impl AsRef<Path>,
    value: &T,
) -> DebugHarnessResult<()> {
    let path = path.as_ref();
    let json =
        serde_json::to_string_pretty(value).map_err(|source| DebugHarnessError::SerializeJson {
            path: path.to_path_buf(),
            source,
        })?;
    write_text_file(path, json)
}

pub fn write_rgb_surface_png(
    path: impl AsRef<Path>,
    surface: &[u32],
    width: u32,
    height: u32,
) -> DebugHarnessResult<()> {
    write_rgb_surface_to_png(path.as_ref(), surface, width, height).map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde::Serialize;

    use super::{step_until_frame_ready, write_json_summary};

    #[derive(Serialize)]
    struct JsonFixture<'a> {
        name: &'a str,
        frames: u64,
    }

    #[test]
    fn step_until_frame_ready_rejects_zero_cycle_step() {
        let mut emulator = latchboy_core::Emulator::new();
        let error = step_until_frame_ready(&mut emulator, 0).expect_err("zero step is invalid");
        assert!(error
            .to_string()
            .contains("cycle_step must be greater than zero"));
    }

    #[test]
    fn write_json_summary_emits_pretty_json_file() {
        let path = std::env::temp_dir().join(format!(
            "latchboy-debug-harness-{}-{}.json",
            std::process::id(),
            "summary"
        ));
        let fixture = JsonFixture {
            name: "debug",
            frames: 2,
        };

        write_json_summary(&path, &fixture).expect("json summary should write");

        let contents = fs::read_to_string(&path).expect("json summary should be readable");
        let _ = fs::remove_file(&path);
        assert!(contents.contains("\n  \"name\": \"debug\""));
        assert!(contents.contains("\n  \"frames\": 2"));
    }
}
