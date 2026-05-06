use super::{load_save_data, EXTERNAL_RAM_START};
use crate::cartridge::save_data::SaveDataError;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(in crate::cartridge) struct RomOnly {
    rom: Vec<u8>,
    external_ram: Option<Vec<u8>>,
}

impl RomOnly {
    pub(super) fn new(rom: Vec<u8>, external_ram: Option<Vec<u8>>) -> Self {
        Self { rom, external_ram }
    }
    pub(super) fn read_rom(&self, address: u16) -> u8 {
        self.rom.get(address as usize).copied().unwrap_or(0xFF)
    }
    pub(super) fn read_ram(&self, address: u16) -> u8 {
        self.external_ram
            .as_ref()
            .and_then(|ram| ram.get((address - EXTERNAL_RAM_START) as usize))
            .copied()
            .unwrap_or(0xFF)
    }
    pub(super) const fn write_control(&mut self, _address: u16, _value: u8) {}
    pub(super) fn write_ram(&mut self, address: u16, value: u8) {
        if let Some(ram) = &mut self.external_ram {
            if let Some(slot) = ram.get_mut((address - EXTERNAL_RAM_START) as usize) {
                *slot = value;
            }
        }
    }
    pub(super) const fn reset(&mut self) {}
    pub(super) fn save_data(&self) -> Option<Vec<u8>> {
        self.external_ram.clone()
    }
    pub(super) fn load_save_data(&mut self, save_data: &[u8]) -> Result<(), SaveDataError> {
        load_save_data(&mut self.external_ram, save_data)
    }
    #[cfg(test)]
    pub(super) fn rom(&self) -> &[u8] {
        &self.rom
    }
}
