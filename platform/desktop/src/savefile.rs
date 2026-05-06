use std::fs;
use std::io;
use std::io::Read;
use std::io::Write;
use std::path::{Path, PathBuf};

use atomic_write_file::AtomicWriteFile;
use latchboy_core::cartridge::{Cartridge, SaveDataError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveLoadStatus {
    NotBatteryBacked,
    NotFound,
    Loaded,
    InvalidData,
    ReadError,
}

pub const fn should_persist_after_load(status: SaveLoadStatus) -> bool {
    matches!(status, SaveLoadStatus::Loaded | SaveLoadStatus::NotFound)
}

/// Derives a deterministic save path for a ROM file.
pub fn save_path_from_rom_path(rom_path: &Path) -> PathBuf {
    rom_path.with_extension("sav")
}

/// Loads save data into a cartridge when battery-backed RAM is present.
pub fn load_save_data_if_available(cartridge: &mut Cartridge, save_path: &Path) -> SaveLoadStatus {
    if !cartridge.has_battery_backed_ram() {
        return SaveLoadStatus::NotBatteryBacked;
    }

    let expected_save_size = cartridge.save_data().map_or(0, |save_data| save_data.len());
    let actual_file_size = match fs::metadata(save_path) {
        Ok(metadata) => metadata.len() as usize,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return SaveLoadStatus::NotFound,
        Err(error) => {
            eprintln!(
                "warning: failed to inspect save file '{}': {error}",
                save_path.display()
            );
            return SaveLoadStatus::ReadError;
        }
    };

    if actual_file_size != expected_save_size {
        eprintln!(
            "warning: save file '{}' size mismatch (expected {expected_save_size} bytes, got {actual_file_size}); continuing with zeroed RAM",
            save_path.display()
        );
        return SaveLoadStatus::InvalidData;
    }

    let save_data = match read_exact_save(save_path, expected_save_size) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return SaveLoadStatus::NotFound,
        Err(error) => {
            eprintln!(
                "warning: failed to read save file '{}': {error}",
                save_path.display()
            );
            return SaveLoadStatus::ReadError;
        }
    };

    match cartridge.load_save_data(&save_data) {
        Ok(()) => SaveLoadStatus::Loaded,
        Err(error) => {
            match error {
                SaveDataError::SizeMismatch {
                    expected_size,
                    actual_size,
                } => {
                    eprintln!(
                        "warning: save file '{}' size mismatch (expected {expected_size} bytes, got {actual_size}); continuing with zeroed RAM",
                        save_path.display()
                    );
                }
                SaveDataError::NoExternalRam | SaveDataError::NotBatteryBackedRam => {
                    eprintln!(
                        "warning: save file '{}' ignored: cartridge cannot accept save data ({error:?})",
                        save_path.display()
                    );
                }
            }
            SaveLoadStatus::InvalidData
        }
    }
}

fn read_exact_save(path: &Path, expected_size: usize) -> io::Result<Vec<u8>> {
    let mut file = fs::File::open(path)?;
    let mut save_data = vec![0; expected_size];
    file.read_exact(&mut save_data)?;
    Ok(save_data)
}

/// Persists battery-backed cartridge save data to disk via an atomic rename.
pub fn persist_save_data(cartridge: &Cartridge, save_path: &Path) {
    let Some(save_data) = cartridge.save_data() else {
        return;
    };

    if let Err(error) = write_atomic(save_path, &save_data) {
        eprintln!(
            "warning: failed to write save file '{}': {error}",
            save_path.display()
        );
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut file = AtomicWriteFile::open(path)?;
    file.write_all(bytes)?;
    file.commit()
}
