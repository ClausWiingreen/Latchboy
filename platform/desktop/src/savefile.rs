use std::fs;
use std::io;
use std::io::Read;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

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

    let mut temp_path = path.to_path_buf();
    let unique = format!(
        "{}.{}.tmp",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    );
    let extension = match path.extension().and_then(|ext| ext.to_str()) {
        Some(ext) if !ext.is_empty() => format!("{ext}.{unique}"),
        _ => unique,
    };
    temp_path.set_extension(extension);

    let mut temp_file = fs::File::create(&temp_path)?;
    temp_file.write_all(bytes)?;
    temp_file.sync_all()?;
    drop(temp_file);

    let replace_result = match fs::rename(&temp_path, path) {
        Ok(()) => Ok(()),
        Err(rename_error) => replace_via_backup(path, &temp_path, rename_error),
    };

    replace_result?;
    sync_parent_directory(path)
}

fn replace_via_backup(path: &Path, temp_path: &Path, rename_error: io::Error) -> io::Result<()> {
    if !path.exists() {
        let _ = fs::remove_file(temp_path);
        return Err(rename_error);
    }

    let mut backup_path = path.to_path_buf();
    let unique = format!(
        "{}.{}.bak",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    );
    let extension = match path.extension().and_then(|ext| ext.to_str()) {
        Some(ext) if !ext.is_empty() => format!("{ext}.{unique}"),
        _ => unique,
    };
    backup_path.set_extension(extension);

    fs::rename(path, &backup_path)?;

    match fs::rename(temp_path, path) {
        Ok(()) => {
            let _ = fs::remove_file(&backup_path);
            Ok(())
        }
        Err(error) => {
            let _ = fs::rename(&backup_path, path);
            let _ = fs::remove_file(temp_path);
            Err(error)
        }
    }
}

fn sync_parent_directory(path: &Path) -> io::Result<()> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };

    #[cfg(unix)]
    {
        let directory = fs::File::open(parent)?;
        directory.sync_all()
    }

    #[cfg(not(unix))]
    {
        let _ = parent;
        Ok(())
    }
}
