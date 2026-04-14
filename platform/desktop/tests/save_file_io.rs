use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use latchboy_core::cartridge::{Cartridge, CartridgeType, DestinationCode, RomSize};
use latchboy_desktop::savefile::{
    load_save_data_if_available, persist_save_data, save_path_from_rom_path,
    should_persist_after_load, SaveLoadStatus,
};

const CARTRIDGE_TYPE_OFFSET: usize = 0x0147;
const ROM_SIZE_OFFSET: usize = 0x0148;
const RAM_SIZE_OFFSET: usize = 0x0149;
const DESTINATION_OFFSET: usize = 0x014A;
const HEADER_CHECKSUM_START: usize = 0x0134;
const HEADER_CHECKSUM_END_INCLUSIVE: usize = 0x014C;
const HEADER_CHECKSUM_OFFSET: usize = 0x014D;

#[test]
fn derives_deterministic_save_path_from_rom_path() {
    let rom_path = Path::new("/tmp/roms/Pokemon.Red.gb");
    let save_path = save_path_from_rom_path(rom_path);
    assert_eq!(save_path, Path::new("/tmp/roms/Pokemon.Red.sav"));
}

#[test]
fn save_file_round_trips_battery_backed_ram() {
    let temp_dir = create_temp_dir();
    let rom_path = temp_dir.join("test.gb");
    let save_path = save_path_from_rom_path(&rom_path);

    let rom = build_rom(CartridgeType::RomRamBattery.code(), 0x02);
    fs::write(&rom_path, &rom).expect("rom should be written for path derivation context");

    let mut cartridge = Cartridge::from_rom(rom.clone()).expect("cartridge should load");
    cartridge.write(0xA000, 0xAB);
    cartridge.write(0xA001, 0xCD);
    persist_save_data(&cartridge, &save_path);

    let mut reloaded = Cartridge::from_rom(rom).expect("reloaded cartridge should load");
    let load_status = load_save_data_if_available(&mut reloaded, &save_path);
    assert_eq!(load_status, SaveLoadStatus::Loaded);

    assert_eq!(reloaded.read(0xA000), 0xAB);
    assert_eq!(reloaded.read(0xA001), 0xCD);

    fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}

#[test]
fn load_ignores_corrupt_size_mismatch_and_leaves_ram_zeroed() {
    let temp_dir = create_temp_dir();
    let rom_path = temp_dir.join("corrupt.gb");
    let save_path = save_path_from_rom_path(&rom_path);
    let rom = build_rom(CartridgeType::RomRamBattery.code(), 0x02);

    fs::write(&save_path, [0x11, 0x22, 0x33]).expect("corrupt save file should be written");

    let mut cartridge = Cartridge::from_rom(rom).expect("cartridge should load");
    let load_status = load_save_data_if_available(&mut cartridge, &save_path);
    assert_eq!(load_status, SaveLoadStatus::InvalidData);

    assert_eq!(cartridge.read(0xA000), 0x00);
    assert_eq!(cartridge.read(0xA001), 0x00);

    fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}

#[test]
fn invalid_save_load_can_be_used_to_skip_persist_and_preserve_original_file() {
    let temp_dir = create_temp_dir();
    let save_path = temp_dir.join("preserve.sav");
    let rom = build_rom(CartridgeType::RomRamBattery.code(), 0x02);
    let original = vec![0x42, 0x24, 0x11];
    fs::write(&save_path, &original).expect("initial mismatched save should be written");

    let mut cartridge = Cartridge::from_rom(rom).expect("cartridge should load");
    let load_status = load_save_data_if_available(&mut cartridge, &save_path);
    let persist_enabled = should_persist_after_load(load_status);
    if persist_enabled {
        persist_save_data(&cartridge, &save_path);
    }

    let after = fs::read(&save_path).expect("save file should remain readable");
    assert_eq!(after, original);

    fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}

#[test]
fn read_error_load_status_disables_persist_gate() {
    let temp_dir = create_temp_dir();
    let save_path = temp_dir.join("locked.sav");
    fs::create_dir_all(&save_path).expect("directory path should trigger read error");

    let rom = build_rom(CartridgeType::RomRamBattery.code(), 0x02);
    let mut cartridge = Cartridge::from_rom(rom).expect("cartridge should load");

    let load_status = load_save_data_if_available(&mut cartridge, &save_path);
    assert_eq!(load_status, SaveLoadStatus::ReadError);
    assert!(!should_persist_after_load(load_status));

    fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}

fn create_temp_dir() -> PathBuf {
    let mut dir = std::env::temp_dir();
    let unique = format!(
        "latchboy-desktop-test-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    );
    dir.push(unique);
    fs::create_dir_all(&dir).expect("temp dir should be created");
    dir
}

fn build_rom(cartridge_type: u8, ram_size_code: u8) -> Vec<u8> {
    let mut rom = vec![0u8; RomSize::Banks2.to_bytes().expect("known rom size")];
    rom[CARTRIDGE_TYPE_OFFSET] = cartridge_type;
    rom[ROM_SIZE_OFFSET] = RomSize::Banks2.code();
    rom[RAM_SIZE_OFFSET] = ram_size_code;
    rom[DESTINATION_OFFSET] = DestinationCode::Japanese.code();
    rom[HEADER_CHECKSUM_OFFSET] = compute_header_checksum(&rom);
    rom
}

fn compute_header_checksum(rom: &[u8]) -> u8 {
    let mut checksum = 0u8;
    for &byte in &rom[HEADER_CHECKSUM_START..=HEADER_CHECKSUM_END_INCLUSIVE] {
        checksum = checksum.wrapping_sub(byte).wrapping_sub(1);
    }
    checksum
}
