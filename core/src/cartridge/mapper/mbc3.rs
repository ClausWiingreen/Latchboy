use super::{load_save_data, ram_offset, RAM_BANK_SIZE, ROM_BANK_SIZE};
use crate::cartridge::save_data::SaveDataError;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(in crate::cartridge) struct Mbc3 {
    rom: Vec<u8>,
    external_ram: Option<Vec<u8>>,
    ram_enabled: bool,
    rom_bank: u8,
    ram_bank_or_rtc: u8,
}

impl Mbc3 {
    pub(super) fn new(rom: Vec<u8>, external_ram: Option<Vec<u8>>) -> Self {
        let mut mapper = Self {
            rom,
            external_ram,
            ram_enabled: false,
            rom_bank: 1,
            ram_bank_or_rtc: 0,
        };
        mapper.reset();
        mapper
    }
    pub(super) fn read_rom(&self, address: u16) -> u8 {
        let rom_bank_count = self.rom.len() / ROM_BANK_SIZE;
        if rom_bank_count == 0 {
            return 0xFF;
        }
        match address {
            0x0000..=0x3FFF => self.rom.get(address as usize).copied().unwrap_or(0xFF),
            0x4000..=0x7FFF => {
                let bank = (self.rom_bank as usize % rom_bank_count).max(1);
                self.rom
                    .get(bank * ROM_BANK_SIZE + (address as usize - ROM_BANK_SIZE))
                    .copied()
                    .unwrap_or(0xFF)
            }
            _ => 0xFF,
        }
    }
    pub(super) fn read_ram(&self, address: u16) -> u8 {
        if !self.ram_enabled || self.ram_bank_or_rtc > 0x03 {
            return 0xFF;
        }
        self.external_ram
            .as_ref()
            .and_then(|ram| ram.get(self.ram_offset(address, ram.len())))
            .copied()
            .unwrap_or(0xFF)
    }
    pub(super) fn write_control(&mut self, address: u16, value: u8) {
        match address {
            0x0000..=0x1FFF => self.ram_enabled = value & 0x0F == 0x0A,
            0x2000..=0x3FFF => {
                let selected = value & 0x7F;
                self.rom_bank = if selected == 0 { 1 } else { selected };
            }
            0x4000..=0x5FFF => self.ram_bank_or_rtc = value & 0x0F,
            0x6000..=0x7FFF => { /* RTC latch unsupported in this phase. */ }
            _ => {}
        }
    }
    pub(super) fn write_ram(&mut self, address: u16, value: u8) {
        if !self.ram_enabled || self.ram_bank_or_rtc > 0x03 {
            return;
        }
        let ram_bank_or_rtc = self.ram_bank_or_rtc;
        if let Some(ram) = &mut self.external_ram {
            let offset = mbc3_ram_offset(address, ram_bank_or_rtc, ram.len());
            if let Some(slot) = ram.get_mut(offset) {
                *slot = value;
            }
        }
    }
    pub(super) fn reset(&mut self) {
        self.ram_enabled = false;
        self.rom_bank = 1;
        self.ram_bank_or_rtc = 0;
    }
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
    fn ram_offset(&self, address: u16, ram_len: usize) -> usize {
        mbc3_ram_offset(address, self.ram_bank_or_rtc, ram_len)
    }
}

fn mbc3_ram_offset(address: u16, ram_bank_or_rtc: u8, ram_len: usize) -> usize {
    let ram_bank_count = (ram_len / RAM_BANK_SIZE).max(1);
    ram_offset(address, (ram_bank_or_rtc as usize) % ram_bank_count)
}
