use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use latchboy_core::cartridge::{Cartridge, SaveDataError};

/// Derives a deterministic save path for a ROM file.
pub fn save_path_from_rom_path(rom_path: &Path) -> PathBuf {
    rom_path.with_extension("sav")
}

/// Loads save data into a cartridge when battery-backed RAM is present.
pub fn load_save_data_if_available(cartridge: &mut Cartridge, save_path: &Path) {
    if !cartridge.has_battery_backed_ram() {
        return;
    }

    let save_data = match fs::read(save_path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return,
        Err(error) => {
            eprintln!(
                "warning: failed to read save file '{}': {error}",
                save_path.display()
            );
            return;
        }
    };

    if let Err(error) = cartridge.load_save_data(&save_data) {
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
    }
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

    fs::write(&temp_path, bytes)?;

    match fs::rename(&temp_path, path) {
        Ok(()) => Ok(()),
        Err(error) => {
            if path.exists() {
                fs::remove_file(path)?;
                fs::rename(&temp_path, path)?;
                Ok(())
            } else {
                let _ = fs::remove_file(&temp_path);
                Err(error)
            }
        }
    }
}
