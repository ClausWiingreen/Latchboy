mod mbc1;
mod mbc3;
mod mbc5;
mod rom_only;

use super::header::CartridgeType;
use super::save_data::SaveDataError;
use super::CartridgeError;

pub(super) const ROM_BANK_SIZE: usize = 0x4000;
pub(super) const RAM_BANK_SIZE: usize = 0x2000;
pub(super) const EXTERNAL_RAM_START: u16 = 0xA000;

use mbc1::Mbc1;
use mbc3::Mbc3;
use mbc5::Mbc5;
use rom_only::RomOnly;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) enum Mapper {
    RomOnly(RomOnly),
    Mbc1(Mbc1),
    Mbc3(Mbc3),
    Mbc5(Mbc5),
}

impl Mapper {
    pub(super) fn new(
        cartridge_type: CartridgeType,
        rom: Vec<u8>,
        external_ram: Option<Vec<u8>>,
    ) -> Result<Self, CartridgeError> {
        match cartridge_type {
            CartridgeType::RomOnly | CartridgeType::RomRam | CartridgeType::RomRamBattery => {
                Ok(Self::RomOnly(RomOnly::new(rom, external_ram)))
            }
            CartridgeType::Mbc1 | CartridgeType::Mbc1Ram | CartridgeType::Mbc1RamBattery => {
                Ok(Self::Mbc1(Mbc1::new(rom, external_ram)))
            }
            CartridgeType::Mbc3
            | CartridgeType::Mbc3Ram
            | CartridgeType::Mbc3RamBattery
            | CartridgeType::Mbc3TimerBattery
            | CartridgeType::Mbc3TimerRamBattery => Ok(Self::Mbc3(Mbc3::new(rom, external_ram))),
            CartridgeType::Mbc5
            | CartridgeType::Mbc5Ram
            | CartridgeType::Mbc5RamBattery
            | CartridgeType::Mbc5Rumble
            | CartridgeType::Mbc5RumbleRam
            | CartridgeType::Mbc5RumbleRamBattery => Ok(Self::Mbc5(Mbc5::new(rom, external_ram))),
            unsupported => Err(CartridgeError::UnsupportedCartridgeType(unsupported)),
        }
    }

    pub(super) fn read_rom(&self, address: u16) -> u8 {
        match self {
            Self::RomOnly(m) => m.read_rom(address),
            Self::Mbc1(m) => m.read_rom(address),
            Self::Mbc3(m) => m.read_rom(address),
            Self::Mbc5(m) => m.read_rom(address),
        }
    }
    pub(super) fn read_ram(&self, address: u16) -> u8 {
        match self {
            Self::RomOnly(m) => m.read_ram(address),
            Self::Mbc1(m) => m.read_ram(address),
            Self::Mbc3(m) => m.read_ram(address),
            Self::Mbc5(m) => m.read_ram(address),
        }
    }
    pub(super) fn write_control(&mut self, address: u16, value: u8) {
        match self {
            Self::RomOnly(m) => m.write_control(address, value),
            Self::Mbc1(m) => m.write_control(address, value),
            Self::Mbc3(m) => m.write_control(address, value),
            Self::Mbc5(m) => m.write_control(address, value),
        }
    }
    pub(super) fn write_ram(&mut self, address: u16, value: u8) {
        match self {
            Self::RomOnly(m) => m.write_ram(address, value),
            Self::Mbc1(m) => m.write_ram(address, value),
            Self::Mbc3(m) => m.write_ram(address, value),
            Self::Mbc5(m) => m.write_ram(address, value),
        }
    }
    pub(super) fn reset(&mut self) {
        match self {
            Self::RomOnly(m) => m.reset(),
            Self::Mbc1(m) => m.reset(),
            Self::Mbc3(m) => m.reset(),
            Self::Mbc5(m) => m.reset(),
        }
    }
    pub(super) fn save_data(&self) -> Option<Vec<u8>> {
        match self {
            Self::RomOnly(m) => m.save_data(),
            Self::Mbc1(m) => m.save_data(),
            Self::Mbc3(m) => m.save_data(),
            Self::Mbc5(m) => m.save_data(),
        }
    }
    pub(super) fn load_save_data(&mut self, save_data: &[u8]) -> Result<(), SaveDataError> {
        match self {
            Self::RomOnly(m) => m.load_save_data(save_data),
            Self::Mbc1(m) => m.load_save_data(save_data),
            Self::Mbc3(m) => m.load_save_data(save_data),
            Self::Mbc5(m) => m.load_save_data(save_data),
        }
    }
    #[cfg(test)]
    pub(super) fn rom(&self) -> &[u8] {
        match self {
            Self::RomOnly(m) => m.rom(),
            Self::Mbc1(m) => m.rom(),
            Self::Mbc3(m) => m.rom(),
            Self::Mbc5(m) => m.rom(),
        }
    }
}

fn ram_offset(address: u16, bank: usize) -> usize {
    bank * RAM_BANK_SIZE + (address as usize - EXTERNAL_RAM_START as usize)
}

fn load_save_data(
    external_ram: &mut Option<Vec<u8>>,
    save_data: &[u8],
) -> Result<(), SaveDataError> {
    let external_ram = external_ram.as_mut().ok_or(SaveDataError::NoExternalRam)?;
    if external_ram.len() != save_data.len() {
        return Err(SaveDataError::SizeMismatch {
            expected_size: external_ram.len(),
            actual_size: save_data.len(),
        });
    }
    external_ram.copy_from_slice(save_data);
    Ok(())
}
