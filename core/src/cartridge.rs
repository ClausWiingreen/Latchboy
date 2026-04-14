const CARTRIDGE_HEADER_SIZE: usize = 0x150;
const TITLE_START: usize = 0x0134;
const TITLE_END_INCLUSIVE: usize = 0x0142;
const HEADER_CHECKSUM_START: usize = 0x0134;
const HEADER_CHECKSUM_END_INCLUSIVE: usize = 0x014C;
const CARTRIDGE_TYPE_OFFSET: usize = 0x0147;
const ROM_SIZE_OFFSET: usize = 0x0148;
const RAM_SIZE_OFFSET: usize = 0x0149;
const DESTINATION_OFFSET: usize = 0x014A;
const HEADER_CHECKSUM_OFFSET: usize = 0x014D;
const ROM_BANK_SIZE: usize = 0x4000;
const RAM_BANK_SIZE: usize = 0x2000;
const EXTERNAL_RAM_START: u16 = 0xA000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CartridgeError {
    RomTooSmall {
        actual_size: usize,
    },
    RomSizeMismatch {
        expected_size: usize,
        actual_size: usize,
    },
    UnsupportedCartridgeType(CartridgeType),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveDataError {
    NoExternalRam,
    SizeMismatch {
        expected_size: usize,
        actual_size: usize,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mapper {
    RomOnly,
    Mbc1 {
        ram_enabled: bool,
        rom_bank_low5: u8,
        bank_upper2: u8,
        banking_mode: u8,
    },
    Mbc3 {
        ram_enabled: bool,
        rom_bank: u8,
        ram_bank_or_rtc: u8,
    },
    Mbc5 {
        ram_enabled: bool,
        rom_bank_low8: u8,
        rom_bank_high1: u8,
        ram_bank: u8,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeaderWarning {
    HeaderChecksumMismatch { expected: u8, actual: u8 },
    UnknownCartridgeType(u8),
    UnknownRomSizeCode(u8),
    UnknownRamSizeCode(u8),
    UnknownDestinationCode(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CartridgeType {
    RomOnly,
    RomRam,
    RomRamBattery,
    Mbc1,
    Mbc1Ram,
    Mbc1RamBattery,
    Mbc3TimerBattery,
    Mbc3TimerRamBattery,
    Mbc3,
    Mbc3Ram,
    Mbc3RamBattery,
    Mbc5,
    Mbc5Ram,
    Mbc5RamBattery,
    Mbc5Rumble,
    Mbc5RumbleRam,
    Mbc5RumbleRamBattery,
    Unknown(u8),
}

impl CartridgeType {
    fn from_code(value: u8) -> Self {
        match value {
            0x00 => Self::RomOnly,
            0x08 => Self::RomRam,
            0x09 => Self::RomRamBattery,
            0x01 => Self::Mbc1,
            0x02 => Self::Mbc1Ram,
            0x03 => Self::Mbc1RamBattery,
            0x0F => Self::Mbc3TimerBattery,
            0x10 => Self::Mbc3TimerRamBattery,
            0x11 => Self::Mbc3,
            0x12 => Self::Mbc3Ram,
            0x13 => Self::Mbc3RamBattery,
            0x19 => Self::Mbc5,
            0x1A => Self::Mbc5Ram,
            0x1B => Self::Mbc5RamBattery,
            0x1C => Self::Mbc5Rumble,
            0x1D => Self::Mbc5RumbleRam,
            0x1E => Self::Mbc5RumbleRamBattery,
            code => Self::Unknown(code),
        }
    }

    pub const fn code(self) -> u8 {
        match self {
            Self::RomOnly => 0x00,
            Self::RomRam => 0x08,
            Self::RomRamBattery => 0x09,
            Self::Mbc1 => 0x01,
            Self::Mbc1Ram => 0x02,
            Self::Mbc1RamBattery => 0x03,
            Self::Mbc3TimerBattery => 0x0F,
            Self::Mbc3TimerRamBattery => 0x10,
            Self::Mbc3 => 0x11,
            Self::Mbc3Ram => 0x12,
            Self::Mbc3RamBattery => 0x13,
            Self::Mbc5 => 0x19,
            Self::Mbc5Ram => 0x1A,
            Self::Mbc5RamBattery => 0x1B,
            Self::Mbc5Rumble => 0x1C,
            Self::Mbc5RumbleRam => 0x1D,
            Self::Mbc5RumbleRamBattery => 0x1E,
            Self::Unknown(value) => value,
        }
    }

    pub const fn has_battery(self) -> bool {
        matches!(
            self,
            Self::RomRamBattery
                | Self::Mbc1RamBattery
                | Self::Mbc3TimerBattery
                | Self::Mbc3TimerRamBattery
                | Self::Mbc3RamBattery
                | Self::Mbc5RamBattery
                | Self::Mbc5RumbleRamBattery
        )
    }

    pub const fn has_battery_backed_ram(self) -> bool {
        matches!(
            self,
            Self::RomRamBattery
                | Self::Mbc1RamBattery
                | Self::Mbc3TimerRamBattery
                | Self::Mbc3RamBattery
                | Self::Mbc5RamBattery
                | Self::Mbc5RumbleRamBattery
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RomSize {
    Banks2,
    Banks4,
    Banks8,
    Banks16,
    Banks32,
    Banks64,
    Banks128,
    Banks256,
    Banks512,
    Banks72,
    Banks80,
    Banks96,
    Unknown(u8),
}

impl RomSize {
    fn from_code(value: u8) -> Self {
        match value {
            0x00 => Self::Banks2,
            0x01 => Self::Banks4,
            0x02 => Self::Banks8,
            0x03 => Self::Banks16,
            0x04 => Self::Banks32,
            0x05 => Self::Banks64,
            0x06 => Self::Banks128,
            0x07 => Self::Banks256,
            0x08 => Self::Banks512,
            0x52 => Self::Banks72,
            0x53 => Self::Banks80,
            0x54 => Self::Banks96,
            code => Self::Unknown(code),
        }
    }

    pub const fn code(self) -> u8 {
        match self {
            Self::Banks2 => 0x00,
            Self::Banks4 => 0x01,
            Self::Banks8 => 0x02,
            Self::Banks16 => 0x03,
            Self::Banks32 => 0x04,
            Self::Banks64 => 0x05,
            Self::Banks128 => 0x06,
            Self::Banks256 => 0x07,
            Self::Banks512 => 0x08,
            Self::Banks72 => 0x52,
            Self::Banks80 => 0x53,
            Self::Banks96 => 0x54,
            Self::Unknown(value) => value,
        }
    }

    pub const fn to_bytes(self) -> Option<usize> {
        match self {
            Self::Banks2 => Some(2 * 16 * 1024),
            Self::Banks4 => Some(4 * 16 * 1024),
            Self::Banks8 => Some(8 * 16 * 1024),
            Self::Banks16 => Some(16 * 16 * 1024),
            Self::Banks32 => Some(32 * 16 * 1024),
            Self::Banks64 => Some(64 * 16 * 1024),
            Self::Banks128 => Some(128 * 16 * 1024),
            Self::Banks256 => Some(256 * 16 * 1024),
            Self::Banks512 => Some(512 * 16 * 1024),
            Self::Banks72 => Some(72 * 16 * 1024),
            Self::Banks80 => Some(80 * 16 * 1024),
            Self::Banks96 => Some(96 * 16 * 1024),
            Self::Unknown(_) => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RamSize {
    None,
    KibiBytes8,
    KibiBytes32,
    KibiBytes64,
    KibiBytes128,
    Unknown(u8),
}

impl RamSize {
    fn from_code(value: u8) -> Self {
        match value {
            0x00 => Self::None,
            0x02 => Self::KibiBytes8,
            0x03 => Self::KibiBytes32,
            0x04 => Self::KibiBytes128,
            0x05 => Self::KibiBytes64,
            code => Self::Unknown(code),
        }
    }

    pub const fn code(self) -> u8 {
        match self {
            Self::None => 0x00,
            Self::KibiBytes8 => 0x02,
            Self::KibiBytes32 => 0x03,
            Self::KibiBytes128 => 0x04,
            Self::KibiBytes64 => 0x05,
            Self::Unknown(value) => value,
        }
    }

    pub const fn to_bytes(self) -> Option<usize> {
        match self {
            Self::None => None,
            Self::KibiBytes8 => Some(8 * 1024),
            Self::KibiBytes32 => Some(32 * 1024),
            Self::KibiBytes64 => Some(64 * 1024),
            Self::KibiBytes128 => Some(128 * 1024),
            Self::Unknown(_) => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DestinationCode {
    Japanese,
    NonJapanese,
    Unknown(u8),
}

impl DestinationCode {
    fn from_code(value: u8) -> Self {
        match value {
            0x00 => Self::Japanese,
            0x01 => Self::NonJapanese,
            code => Self::Unknown(code),
        }
    }

    pub const fn code(self) -> u8 {
        match self {
            Self::Japanese => 0x00,
            Self::NonJapanese => 0x01,
            Self::Unknown(value) => value,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CartridgeHeader {
    pub title: String,
    pub cartridge_type: CartridgeType,
    pub rom_size: RomSize,
    pub ram_size: RamSize,
    pub destination_code: DestinationCode,
    pub header_checksum: u8,
    pub computed_header_checksum: u8,
}

impl CartridgeHeader {
    pub fn parse(rom: &[u8]) -> Result<Self, CartridgeError> {
        if rom.len() < CARTRIDGE_HEADER_SIZE {
            return Err(CartridgeError::RomTooSmall {
                actual_size: rom.len(),
            });
        }

        let title_bytes = &rom[TITLE_START..=TITLE_END_INCLUSIVE];
        let title_end = title_bytes
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(title_bytes.len());
        let title = String::from_utf8_lossy(&title_bytes[..title_end]).into_owned();

        let header_checksum = rom[HEADER_CHECKSUM_OFFSET];

        let computed_header_checksum = compute_header_checksum(rom)?;

        Ok(Self {
            title,
            cartridge_type: CartridgeType::from_code(rom[CARTRIDGE_TYPE_OFFSET]),
            rom_size: RomSize::from_code(rom[ROM_SIZE_OFFSET]),
            ram_size: RamSize::from_code(rom[RAM_SIZE_OFFSET]),
            destination_code: DestinationCode::from_code(rom[DESTINATION_OFFSET]),
            header_checksum,
            computed_header_checksum,
        })
    }

    pub const fn has_valid_header_checksum(&self) -> bool {
        self.header_checksum == self.computed_header_checksum
    }

    pub fn warnings(&self) -> Vec<HeaderWarning> {
        let mut warnings = Vec::new();

        if !self.has_valid_header_checksum() {
            warnings.push(HeaderWarning::HeaderChecksumMismatch {
                expected: self.computed_header_checksum,
                actual: self.header_checksum,
            });
        }

        if let CartridgeType::Unknown(code) = self.cartridge_type {
            warnings.push(HeaderWarning::UnknownCartridgeType(code));
        }

        if let RomSize::Unknown(code) = self.rom_size {
            warnings.push(HeaderWarning::UnknownRomSizeCode(code));
        }

        if let RamSize::Unknown(code) = self.ram_size {
            warnings.push(HeaderWarning::UnknownRamSizeCode(code));
        }

        if let DestinationCode::Unknown(code) = self.destination_code {
            warnings.push(HeaderWarning::UnknownDestinationCode(code));
        }

        warnings
    }
}

pub fn compute_header_checksum(rom: &[u8]) -> Result<u8, CartridgeError> {
    if rom.len() < CARTRIDGE_HEADER_SIZE {
        return Err(CartridgeError::RomTooSmall {
            actual_size: rom.len(),
        });
    }

    Ok(rom[HEADER_CHECKSUM_START..=HEADER_CHECKSUM_END_INCLUSIVE]
        .iter()
        .fold(0u8, |acc, byte| acc.wrapping_sub(*byte).wrapping_sub(1)))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cartridge {
    pub header: CartridgeHeader,
    pub warnings: Vec<HeaderWarning>,
    rom: Vec<u8>,
    external_ram: Option<Vec<u8>>,
    mapper: Mapper,
}

impl Cartridge {
    pub fn from_rom(rom: Vec<u8>) -> Result<Self, CartridgeError> {
        let header = CartridgeHeader::parse(&rom)?;
        if let Some(expected_size) = header.rom_size.to_bytes() {
            if rom.len() < expected_size {
                return Err(CartridgeError::RomSizeMismatch {
                    expected_size,
                    actual_size: rom.len(),
                });
            }
        }

        let mapper = match header.cartridge_type {
            CartridgeType::RomOnly | CartridgeType::RomRam | CartridgeType::RomRamBattery => {
                Mapper::RomOnly
            }
            CartridgeType::Mbc1 | CartridgeType::Mbc1Ram | CartridgeType::Mbc1RamBattery => {
                Mapper::Mbc1 {
                    ram_enabled: false,
                    rom_bank_low5: 1,
                    bank_upper2: 0,
                    banking_mode: 0,
                }
            }
            CartridgeType::Mbc3
            | CartridgeType::Mbc3Ram
            | CartridgeType::Mbc3RamBattery
            | CartridgeType::Mbc3TimerBattery
            | CartridgeType::Mbc3TimerRamBattery => Mapper::Mbc3 {
                ram_enabled: false,
                rom_bank: 1,
                ram_bank_or_rtc: 0,
            },
            CartridgeType::Mbc5
            | CartridgeType::Mbc5Ram
            | CartridgeType::Mbc5RamBattery
            | CartridgeType::Mbc5Rumble
            | CartridgeType::Mbc5RumbleRam
            | CartridgeType::Mbc5RumbleRamBattery => Mapper::Mbc5 {
                ram_enabled: false,
                rom_bank_low8: 1,
                rom_bank_high1: 0,
                ram_bank: 0,
            },
            unsupported => return Err(CartridgeError::UnsupportedCartridgeType(unsupported)),
        };

        let warnings = header.warnings();
        let external_ram = header.ram_size.to_bytes().map(|size| vec![0u8; size]);

        Ok(Self {
            header,
            warnings,
            rom,
            external_ram,
            mapper,
        })
    }

    pub fn read(&self, address: u16) -> u8 {
        match self.mapper {
            Mapper::RomOnly => match address {
                0x0000..=0x7FFF => self.rom.get(address as usize).copied().unwrap_or(0xFF),
                0xA000..=0xBFFF => self
                    .external_ram
                    .as_ref()
                    .and_then(|ram| ram.get((address - 0xA000) as usize))
                    .copied()
                    .unwrap_or(0xFF),
                _ => 0xFF,
            },
            Mapper::Mbc1 {
                ram_enabled,
                rom_bank_low5,
                bank_upper2,
                banking_mode,
            } => self.read_mbc1(
                address,
                ram_enabled,
                rom_bank_low5,
                bank_upper2,
                banking_mode,
            ),
            Mapper::Mbc3 {
                ram_enabled,
                rom_bank,
                ram_bank_or_rtc,
            } => self.read_mbc3(address, ram_enabled, rom_bank, ram_bank_or_rtc),
            Mapper::Mbc5 {
                ram_enabled,
                rom_bank_low8,
                rom_bank_high1,
                ram_bank,
            } => self.read_mbc5(
                address,
                ram_enabled,
                rom_bank_low8,
                rom_bank_high1,
                ram_bank,
            ),
        }
    }

    pub const fn has_battery_backed_ram(&self) -> bool {
        self.header.cartridge_type.has_battery_backed_ram()
    }

    pub fn save_data(&self) -> Option<Vec<u8>> {
        if !self.has_battery_backed_ram() {
            return None;
        }

        self.external_ram.clone()
    }

    pub fn load_save_data(&mut self, save_data: &[u8]) -> Result<(), SaveDataError> {
        let external_ram = self
            .external_ram
            .as_mut()
            .ok_or(SaveDataError::NoExternalRam)?;

        if external_ram.len() != save_data.len() {
            return Err(SaveDataError::SizeMismatch {
                expected_size: external_ram.len(),
                actual_size: save_data.len(),
            });
        }

        external_ram.copy_from_slice(save_data);
        Ok(())
    }

    pub fn write(&mut self, address: u16, value: u8) {
        match &mut self.mapper {
            Mapper::RomOnly => {
                if let 0xA000..=0xBFFF = address {
                    Self::write_external_ram(&mut self.external_ram, address, value);
                }
            }
            Mapper::Mbc1 {
                ram_enabled,
                rom_bank_low5,
                bank_upper2,
                banking_mode,
            } => Self::write_mbc1(
                &mut self.external_ram,
                address,
                value,
                ram_enabled,
                rom_bank_low5,
                bank_upper2,
                banking_mode,
            ),
            Mapper::Mbc3 {
                ram_enabled,
                rom_bank,
                ram_bank_or_rtc,
            } => Self::write_mbc3(
                &mut self.external_ram,
                address,
                value,
                ram_enabled,
                rom_bank,
                ram_bank_or_rtc,
            ),
            Mapper::Mbc5 {
                ram_enabled,
                rom_bank_low8,
                rom_bank_high1,
                ram_bank,
            } => Self::write_mbc5(
                &mut self.external_ram,
                address,
                value,
                ram_enabled,
                rom_bank_low8,
                rom_bank_high1,
                ram_bank,
            ),
        }
    }

    fn write_external_ram(external_ram: &mut Option<Vec<u8>>, address: u16, value: u8) {
        if let Some(ram) = external_ram {
            if let Some(slot) = ram.get_mut((address - EXTERNAL_RAM_START) as usize) {
                *slot = value;
            }
        }
    }

    fn write_mbc1(
        external_ram: &mut Option<Vec<u8>>,
        address: u16,
        value: u8,
        ram_enabled: &mut bool,
        rom_bank_low5: &mut u8,
        bank_upper2: &mut u8,
        banking_mode: &mut u8,
    ) {
        match address {
            0x0000..=0x1FFF => *ram_enabled = value & 0x0F == 0x0A,
            0x2000..=0x3FFF => {
                let selected = value & 0x1F;
                *rom_bank_low5 = if selected == 0 { 1 } else { selected };
            }
            0x4000..=0x5FFF => *bank_upper2 = value & 0x03,
            0x6000..=0x7FFF => *banking_mode = value & 0x01,
            0xA000..=0xBFFF => {
                if !*ram_enabled {
                    return;
                }

                if let Some(ram) = external_ram {
                    let offset =
                        Self::mbc1_ram_offset(address, *banking_mode, *bank_upper2, ram.len());
                    if let Some(slot) = ram.get_mut(offset) {
                        *slot = value;
                    }
                }
            }
            _ => {}
        }
    }

    fn write_mbc3(
        external_ram: &mut Option<Vec<u8>>,
        address: u16,
        value: u8,
        ram_enabled: &mut bool,
        rom_bank: &mut u8,
        ram_bank_or_rtc: &mut u8,
    ) {
        match address {
            0x0000..=0x1FFF => *ram_enabled = value & 0x0F == 0x0A,
            0x2000..=0x3FFF => {
                let selected = value & 0x7F;
                *rom_bank = if selected == 0 { 1 } else { selected };
            }
            0x4000..=0x5FFF => *ram_bank_or_rtc = value & 0x0F,
            0x6000..=0x7FFF => {
                // RTC latch unsupported in this phase.
            }
            0xA000..=0xBFFF => {
                if !*ram_enabled || *ram_bank_or_rtc > 0x03 {
                    return;
                }

                if let Some(ram) = external_ram {
                    let offset = Self::mbc3_ram_offset(address, *ram_bank_or_rtc, ram.len());
                    if let Some(slot) = ram.get_mut(offset) {
                        *slot = value;
                    }
                }
            }
            _ => {}
        }
    }

    fn write_mbc5(
        external_ram: &mut Option<Vec<u8>>,
        address: u16,
        value: u8,
        ram_enabled: &mut bool,
        rom_bank_low8: &mut u8,
        rom_bank_high1: &mut u8,
        ram_bank: &mut u8,
    ) {
        match address {
            0x0000..=0x1FFF => *ram_enabled = value & 0x0F == 0x0A,
            0x2000..=0x2FFF => *rom_bank_low8 = value,
            0x3000..=0x3FFF => *rom_bank_high1 = value & 0x01,
            0x4000..=0x5FFF => *ram_bank = value & 0x0F,
            0xA000..=0xBFFF => {
                if !*ram_enabled {
                    return;
                }

                if let Some(ram) = external_ram {
                    let offset = Self::mbc5_ram_offset(address, *ram_bank, ram.len());
                    if let Some(slot) = ram.get_mut(offset) {
                        *slot = value;
                    }
                }
            }
            _ => {}
        }
    }

    fn read_mbc1(
        &self,
        address: u16,
        ram_enabled: bool,
        rom_bank_low5: u8,
        bank_upper2: u8,
        banking_mode: u8,
    ) -> u8 {
        let rom_bank_count = self.rom.len() / ROM_BANK_SIZE;
        if rom_bank_count == 0 {
            return 0xFF;
        }

        match address {
            0x0000..=0x3FFF => {
                let bank = if banking_mode == 0 {
                    0
                } else {
                    ((bank_upper2 as usize) << 5) % rom_bank_count
                };
                let offset = bank * ROM_BANK_SIZE + address as usize;
                self.rom.get(offset).copied().unwrap_or(0xFF)
            }
            0x4000..=0x7FFF => {
                let bank = ((((bank_upper2 as usize) << 5) | rom_bank_low5 as usize)
                    % rom_bank_count)
                    .max(1);
                let offset = bank * ROM_BANK_SIZE + (address as usize - ROM_BANK_SIZE);
                self.rom.get(offset).copied().unwrap_or(0xFF)
            }
            0xA000..=0xBFFF => {
                if !ram_enabled {
                    return 0xFF;
                }

                self.external_ram
                    .as_ref()
                    .and_then(|ram| {
                        let offset =
                            Self::mbc1_ram_offset(address, banking_mode, bank_upper2, ram.len());
                        ram.get(offset)
                    })
                    .copied()
                    .unwrap_or(0xFF)
            }
            _ => 0xFF,
        }
    }

    fn mbc1_ram_offset(address: u16, banking_mode: u8, bank_upper2: u8, ram_len: usize) -> usize {
        let ram_bank_count = (ram_len / RAM_BANK_SIZE).max(1);
        let bank = if banking_mode == 0 {
            0
        } else {
            (bank_upper2 as usize) % ram_bank_count
        };

        bank * RAM_BANK_SIZE + (address as usize - EXTERNAL_RAM_START as usize)
    }

    fn read_mbc3(&self, address: u16, ram_enabled: bool, rom_bank: u8, ram_bank_or_rtc: u8) -> u8 {
        let rom_bank_count = self.rom.len() / ROM_BANK_SIZE;
        if rom_bank_count == 0 {
            return 0xFF;
        }

        match address {
            0x0000..=0x3FFF => self.rom.get(address as usize).copied().unwrap_or(0xFF),
            0x4000..=0x7FFF => {
                let bank = (rom_bank as usize % rom_bank_count).max(1);
                let offset = bank * ROM_BANK_SIZE + (address as usize - ROM_BANK_SIZE);
                self.rom.get(offset).copied().unwrap_or(0xFF)
            }
            0xA000..=0xBFFF => {
                if !ram_enabled {
                    return 0xFF;
                }

                if ram_bank_or_rtc > 0x03 {
                    return 0xFF;
                }

                self.external_ram
                    .as_ref()
                    .and_then(|ram| {
                        let offset = Self::mbc3_ram_offset(address, ram_bank_or_rtc, ram.len());
                        ram.get(offset)
                    })
                    .copied()
                    .unwrap_or(0xFF)
            }
            _ => 0xFF,
        }
    }

    fn mbc3_ram_offset(address: u16, ram_bank_or_rtc: u8, ram_len: usize) -> usize {
        let ram_bank_count = (ram_len / RAM_BANK_SIZE).max(1);
        let bank = (ram_bank_or_rtc as usize) % ram_bank_count;
        bank * RAM_BANK_SIZE + (address as usize - EXTERNAL_RAM_START as usize)
    }

    fn read_mbc5(
        &self,
        address: u16,
        ram_enabled: bool,
        rom_bank_low8: u8,
        rom_bank_high1: u8,
        ram_bank: u8,
    ) -> u8 {
        let rom_bank_count = self.rom.len() / ROM_BANK_SIZE;
        if rom_bank_count == 0 {
            return 0xFF;
        }

        match address {
            0x0000..=0x3FFF => self.rom.get(address as usize).copied().unwrap_or(0xFF),
            0x4000..=0x7FFF => {
                let bank =
                    (((rom_bank_high1 as usize) << 8) | rom_bank_low8 as usize) % rom_bank_count;
                let offset = bank * ROM_BANK_SIZE + (address as usize - ROM_BANK_SIZE);
                self.rom.get(offset).copied().unwrap_or(0xFF)
            }
            0xA000..=0xBFFF => {
                if !ram_enabled {
                    return 0xFF;
                }

                self.external_ram
                    .as_ref()
                    .and_then(|ram| {
                        let offset = Self::mbc5_ram_offset(address, ram_bank, ram.len());
                        ram.get(offset)
                    })
                    .copied()
                    .unwrap_or(0xFF)
            }
            _ => 0xFF,
        }
    }

    fn mbc5_ram_offset(address: u16, ram_bank: u8, ram_len: usize) -> usize {
        let ram_bank_count = (ram_len / RAM_BANK_SIZE).max(1);
        let bank = (ram_bank as usize) % ram_bank_count;
        bank * RAM_BANK_SIZE + (address as usize - EXTERNAL_RAM_START as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_rom() -> Vec<u8> {
        let mut rom = vec![0u8; CARTRIDGE_HEADER_SIZE];
        let title = b"TETRIS";
        rom[TITLE_START..TITLE_START + title.len()].copy_from_slice(title);
        rom[CARTRIDGE_TYPE_OFFSET] = CartridgeType::RomOnly.code();
        rom[ROM_SIZE_OFFSET] = RomSize::Banks2.code();
        rom[RAM_SIZE_OFFSET] = RamSize::None.code();
        rom[DESTINATION_OFFSET] = DestinationCode::Japanese.code();

        let checksum = compute_header_checksum(&rom).expect("checksum should compute");
        rom[HEADER_CHECKSUM_OFFSET] = checksum;

        rom
    }

    #[test]
    fn parse_extracts_supported_header_fields() {
        let rom = make_test_rom();
        let header = CartridgeHeader::parse(&rom).expect("header should parse");

        assert_eq!(header.title, "TETRIS");
        assert_eq!(header.cartridge_type, CartridgeType::RomOnly);
        assert_eq!(header.rom_size, RomSize::Banks2);
        assert_eq!(header.ram_size, RamSize::None);
        assert_eq!(header.destination_code, DestinationCode::Japanese);
        assert!(header.has_valid_header_checksum());
        assert!(header.warnings().is_empty());
    }

    #[test]
    fn parse_exposes_checksum_warning_when_header_checksum_is_invalid() {
        let mut rom = make_test_rom();
        rom[HEADER_CHECKSUM_OFFSET] = rom[HEADER_CHECKSUM_OFFSET].wrapping_add(1);

        let header = CartridgeHeader::parse(&rom).expect("header should parse");
        let warnings = header.warnings();

        assert!(warnings
            .iter()
            .any(|warning| matches!(warning, HeaderWarning::HeaderChecksumMismatch { .. })));
    }

    #[test]
    fn parse_exposes_unknown_field_warnings() {
        let mut rom = make_test_rom();
        rom[CARTRIDGE_TYPE_OFFSET] = 0xFF;
        rom[ROM_SIZE_OFFSET] = 0xFF;
        rom[RAM_SIZE_OFFSET] = 0xFF;
        rom[DESTINATION_OFFSET] = 0xFF;
        rom[HEADER_CHECKSUM_OFFSET] =
            compute_header_checksum(&rom).expect("checksum should compute");

        let header = CartridgeHeader::parse(&rom).expect("header should parse");
        let warnings = header.warnings();

        assert!(warnings
            .iter()
            .any(|warning| matches!(warning, HeaderWarning::UnknownCartridgeType(0xFF))));
        assert!(warnings
            .iter()
            .any(|warning| matches!(warning, HeaderWarning::UnknownRomSizeCode(0xFF))));
        assert!(warnings
            .iter()
            .any(|warning| matches!(warning, HeaderWarning::UnknownRamSizeCode(0xFF))));
        assert!(warnings
            .iter()
            .any(|warning| matches!(warning, HeaderWarning::UnknownDestinationCode(0xFF))));
    }

    #[test]
    fn parse_rejects_roms_smaller_than_header_region() {
        let rom = vec![0u8; CARTRIDGE_HEADER_SIZE - 1];
        let error = CartridgeHeader::parse(&rom).expect_err("rom should be rejected");

        assert_eq!(
            error,
            CartridgeError::RomTooSmall {
                actual_size: CARTRIDGE_HEADER_SIZE - 1,
            }
        );
    }

    #[test]
    fn compute_header_checksum_rejects_roms_smaller_than_header_region() {
        let rom = vec![0u8; CARTRIDGE_HEADER_SIZE - 1];
        let error = compute_header_checksum(&rom).expect_err("rom should be rejected");

        assert_eq!(
            error,
            CartridgeError::RomTooSmall {
                actual_size: CARTRIDGE_HEADER_SIZE - 1,
            }
        );
    }

    #[test]
    fn parse_does_not_include_cgb_flag_in_title() {
        let mut rom = make_test_rom();
        rom[TITLE_START..=TITLE_END_INCLUSIVE].fill(b'A');
        rom[TITLE_END_INCLUSIVE + 1] = 0x80;
        rom[HEADER_CHECKSUM_OFFSET] =
            compute_header_checksum(&rom).expect("checksum should compute");

        let header = CartridgeHeader::parse(&rom).expect("header should parse");

        assert_eq!(header.title, "AAAAAAAAAAAAAAA");
    }

    #[test]
    fn parse_supports_representative_cartridge_variants() {
        let cases = [
            (
                CartridgeType::RomOnly.code(),
                RomSize::Banks2.code(),
                RamSize::None.code(),
                CartridgeType::RomOnly,
                RomSize::Banks2,
                RamSize::None,
            ),
            (
                CartridgeType::Mbc1.code(),
                RomSize::Banks32.code(),
                RamSize::KibiBytes32.code(),
                CartridgeType::Mbc1,
                RomSize::Banks32,
                RamSize::KibiBytes32,
            ),
            (
                CartridgeType::Mbc3.code(),
                RomSize::Banks64.code(),
                RamSize::KibiBytes32.code(),
                CartridgeType::Mbc3,
                RomSize::Banks64,
                RamSize::KibiBytes32,
            ),
            (
                CartridgeType::Mbc5.code(),
                RomSize::Banks128.code(),
                RamSize::KibiBytes128.code(),
                CartridgeType::Mbc5,
                RomSize::Banks128,
                RamSize::KibiBytes128,
            ),
        ];

        for (
            cartridge_type_code,
            rom_size_code,
            ram_size_code,
            expected_cartridge_type,
            expected_rom_size,
            expected_ram_size,
        ) in cases
        {
            let mut rom = make_test_rom();
            rom[CARTRIDGE_TYPE_OFFSET] = cartridge_type_code;
            rom[ROM_SIZE_OFFSET] = rom_size_code;
            rom[RAM_SIZE_OFFSET] = ram_size_code;
            rom[HEADER_CHECKSUM_OFFSET] =
                compute_header_checksum(&rom).expect("checksum should compute");

            let header = CartridgeHeader::parse(&rom).expect("header should parse");

            assert_eq!(header.cartridge_type, expected_cartridge_type);
            assert_eq!(header.rom_size, expected_rom_size);
            assert_eq!(header.ram_size, expected_ram_size);
            assert!(header.has_valid_header_checksum());
            assert!(header.warnings().is_empty());
        }
    }

    #[test]
    fn cartridge_rom_only_reads_and_ignores_rom_writes() {
        let mut rom = vec![0u8; 0x8000];
        rom[TITLE_START..TITLE_START + 4].copy_from_slice(b"TEST");
        rom[CARTRIDGE_TYPE_OFFSET] = CartridgeType::RomOnly.code();
        rom[ROM_SIZE_OFFSET] = RomSize::Banks2.code();
        rom[RAM_SIZE_OFFSET] = RamSize::None.code();
        rom[DESTINATION_OFFSET] = DestinationCode::NonJapanese.code();
        rom[0x1234] = 0x42;
        rom[HEADER_CHECKSUM_OFFSET] =
            compute_header_checksum(&rom).expect("checksum should compute");

        let mut cartridge = Cartridge::from_rom(rom).expect("rom-only cartridge should load");

        assert_eq!(cartridge.read(0x1234), 0x42);
        cartridge.write(0x1234, 0x99);
        assert_eq!(cartridge.read(0x1234), 0x42);
    }

    #[test]
    fn cartridge_with_external_ram_supports_ram_reads_and_writes() {
        let mut rom = vec![0u8; 0x8000];
        rom[CARTRIDGE_TYPE_OFFSET] = CartridgeType::RomRam.code();
        rom[ROM_SIZE_OFFSET] = RomSize::Banks2.code();
        rom[RAM_SIZE_OFFSET] = RamSize::KibiBytes8.code();
        rom[DESTINATION_OFFSET] = DestinationCode::Japanese.code();
        rom[HEADER_CHECKSUM_OFFSET] =
            compute_header_checksum(&rom).expect("checksum should compute");

        let mut cartridge = Cartridge::from_rom(rom).expect("rom+ram cartridge should load");
        assert_eq!(cartridge.read(0xA123), 0x00);

        cartridge.write(0xA123, 0x77);
        assert_eq!(cartridge.read(0xA123), 0x77);
    }

    #[test]
    fn cartridge_accepts_mbc1() {
        let mut rom = vec![0u8; 0x8000];
        rom[CARTRIDGE_TYPE_OFFSET] = CartridgeType::Mbc1.code();
        rom[ROM_SIZE_OFFSET] = RomSize::Banks2.code();
        rom[RAM_SIZE_OFFSET] = RamSize::None.code();
        rom[DESTINATION_OFFSET] = DestinationCode::Japanese.code();
        rom[HEADER_CHECKSUM_OFFSET] =
            compute_header_checksum(&rom).expect("checksum should compute");

        let cartridge = Cartridge::from_rom(rom).expect("mbc1 should be supported");
        assert_eq!(cartridge.read(0x1000), 0x00);
    }

    #[test]
    fn mbc1_switches_rom_banks() {
        let mut rom = vec![0u8; 0x10000];
        rom[CARTRIDGE_TYPE_OFFSET] = CartridgeType::Mbc1.code();
        rom[ROM_SIZE_OFFSET] = RomSize::Banks4.code();
        rom[RAM_SIZE_OFFSET] = RamSize::None.code();
        rom[DESTINATION_OFFSET] = DestinationCode::Japanese.code();
        rom[HEADER_CHECKSUM_OFFSET] =
            compute_header_checksum(&rom).expect("checksum should compute");
        rom[0x4000] = 0x01;
        rom[0x8000] = 0x02;
        rom[0xC000] = 0x03;

        let mut cartridge = Cartridge::from_rom(rom).expect("mbc1 should be supported");

        assert_eq!(cartridge.read(0x4000), 0x01);
        cartridge.write(0x2000, 0x02);
        assert_eq!(cartridge.read(0x4000), 0x02);
        cartridge.write(0x2000, 0x03);
        assert_eq!(cartridge.read(0x4000), 0x03);
    }

    #[test]
    fn mbc1_supports_ram_enable_and_disable() {
        let mut rom = vec![0u8; 0x8000];
        rom[CARTRIDGE_TYPE_OFFSET] = CartridgeType::Mbc1Ram.code();
        rom[ROM_SIZE_OFFSET] = RomSize::Banks2.code();
        rom[RAM_SIZE_OFFSET] = RamSize::KibiBytes8.code();
        rom[DESTINATION_OFFSET] = DestinationCode::Japanese.code();
        rom[HEADER_CHECKSUM_OFFSET] =
            compute_header_checksum(&rom).expect("checksum should compute");

        let mut cartridge = Cartridge::from_rom(rom).expect("mbc1 should be supported");

        cartridge.write(0xA000, 0x55);
        assert_eq!(cartridge.read(0xA000), 0xFF);

        cartridge.write(0x0000, 0x0A);
        cartridge.write(0xA000, 0x55);
        assert_eq!(cartridge.read(0xA000), 0x55);

        cartridge.write(0x0000, 0x00);
        assert_eq!(cartridge.read(0xA000), 0xFF);
    }

    #[test]
    fn mbc1_uses_upper_bank_bits_in_advanced_banking_mode() {
        let mut rom = vec![0u8; 64 * 0x4000];
        rom[CARTRIDGE_TYPE_OFFSET] = CartridgeType::Mbc1.code();
        rom[ROM_SIZE_OFFSET] = RomSize::Banks64.code();
        rom[RAM_SIZE_OFFSET] = RamSize::None.code();
        rom[DESTINATION_OFFSET] = DestinationCode::Japanese.code();
        rom[HEADER_CHECKSUM_OFFSET] =
            compute_header_checksum(&rom).expect("checksum should compute");

        let bank0_offset = 32 * 0x4000;
        rom[bank0_offset] = 0x20;
        let bank33_offset = 33 * 0x4000;
        rom[bank33_offset] = 0x21;

        let mut cartridge = Cartridge::from_rom(rom).expect("mbc1 should be supported");
        cartridge.write(0x2000, 0x01);
        cartridge.write(0x4000, 0x01);
        cartridge.write(0x6000, 0x01);

        assert_eq!(cartridge.read(0x0000), 0x20);
        assert_eq!(cartridge.read(0x4000), 0x21);
    }

    #[test]
    fn mbc1_clamps_ram_bank_to_available_ram_size() {
        let mut rom = vec![0u8; 0x8000];
        rom[CARTRIDGE_TYPE_OFFSET] = CartridgeType::Mbc1Ram.code();
        rom[ROM_SIZE_OFFSET] = RomSize::Banks2.code();
        rom[RAM_SIZE_OFFSET] = RamSize::KibiBytes8.code();
        rom[DESTINATION_OFFSET] = DestinationCode::Japanese.code();
        rom[HEADER_CHECKSUM_OFFSET] =
            compute_header_checksum(&rom).expect("checksum should compute");

        let mut cartridge = Cartridge::from_rom(rom).expect("mbc1 should be supported");
        cartridge.write(0x0000, 0x0A);
        cartridge.write(0x6000, 0x01);
        cartridge.write(0x4000, 0x03);
        cartridge.write(0xA000, 0x66);

        assert_eq!(cartridge.read(0xA000), 0x66);
    }

    #[test]
    fn cartridge_accepts_mbc3() {
        let mut rom = vec![0u8; 0x10000];
        rom[CARTRIDGE_TYPE_OFFSET] = CartridgeType::Mbc3.code();
        rom[ROM_SIZE_OFFSET] = RomSize::Banks4.code();
        rom[RAM_SIZE_OFFSET] = RamSize::None.code();
        rom[DESTINATION_OFFSET] = DestinationCode::Japanese.code();
        rom[HEADER_CHECKSUM_OFFSET] =
            compute_header_checksum(&rom).expect("checksum should compute");

        let cartridge = Cartridge::from_rom(rom).expect("mbc3 should be supported");
        assert_eq!(cartridge.read(0x1000), 0x00);
    }

    #[test]
    fn mbc3_switches_rom_banks() {
        let mut rom = vec![0u8; 0x10000];
        rom[CARTRIDGE_TYPE_OFFSET] = CartridgeType::Mbc3.code();
        rom[ROM_SIZE_OFFSET] = RomSize::Banks4.code();
        rom[RAM_SIZE_OFFSET] = RamSize::None.code();
        rom[DESTINATION_OFFSET] = DestinationCode::Japanese.code();
        rom[0x4000] = 0x01;
        rom[0x8000] = 0x02;
        rom[0xC000] = 0x03;
        rom[HEADER_CHECKSUM_OFFSET] =
            compute_header_checksum(&rom).expect("checksum should compute");

        let mut cartridge = Cartridge::from_rom(rom).expect("mbc3 should be supported");
        assert_eq!(cartridge.read(0x4000), 0x01);

        cartridge.write(0x2000, 0x02);
        assert_eq!(cartridge.read(0x4000), 0x02);

        cartridge.write(0x2000, 0x03);
        assert_eq!(cartridge.read(0x4000), 0x03);
    }

    #[test]
    fn mbc3_supports_ram_enable_and_bank_switching() {
        let mut rom = vec![0u8; 0x10000];
        rom[CARTRIDGE_TYPE_OFFSET] = CartridgeType::Mbc3Ram.code();
        rom[ROM_SIZE_OFFSET] = RomSize::Banks4.code();
        rom[RAM_SIZE_OFFSET] = RamSize::KibiBytes32.code();
        rom[DESTINATION_OFFSET] = DestinationCode::Japanese.code();
        rom[HEADER_CHECKSUM_OFFSET] =
            compute_header_checksum(&rom).expect("checksum should compute");

        let mut cartridge = Cartridge::from_rom(rom).expect("mbc3 should be supported");

        cartridge.write(0xA000, 0x12);
        assert_eq!(cartridge.read(0xA000), 0xFF);

        cartridge.write(0x0000, 0x0A);
        cartridge.write(0xA000, 0x12);
        assert_eq!(cartridge.read(0xA000), 0x12);

        cartridge.write(0x4000, 0x01);
        cartridge.write(0xA000, 0x34);
        assert_eq!(cartridge.read(0xA000), 0x34);

        cartridge.write(0x4000, 0x81);
        cartridge.write(0xA000, 0x56);
        assert_eq!(cartridge.read(0xA000), 0x56);

        cartridge.write(0x4000, 0x00);
        assert_eq!(cartridge.read(0xA000), 0x12);

        cartridge.write(0x4000, 0x08);
        assert_eq!(cartridge.read(0xA000), 0xFF);
    }

    #[test]
    fn cartridge_accepts_mbc5() {
        let mut rom = vec![0u8; 0x10000];
        rom[CARTRIDGE_TYPE_OFFSET] = CartridgeType::Mbc5.code();
        rom[ROM_SIZE_OFFSET] = RomSize::Banks4.code();
        rom[RAM_SIZE_OFFSET] = RamSize::None.code();
        rom[DESTINATION_OFFSET] = DestinationCode::Japanese.code();
        rom[HEADER_CHECKSUM_OFFSET] =
            compute_header_checksum(&rom).expect("checksum should compute");

        let cartridge = Cartridge::from_rom(rom).expect("mbc5 should be supported");
        assert_eq!(cartridge.read(0x1000), 0x00);
    }

    #[test]
    fn mbc5_switches_rom_banks_with_9_bit_register() {
        let mut rom = vec![0u8; 512 * 0x4000];
        rom[CARTRIDGE_TYPE_OFFSET] = CartridgeType::Mbc5.code();
        rom[ROM_SIZE_OFFSET] = RomSize::Banks512.code();
        rom[RAM_SIZE_OFFSET] = RamSize::None.code();
        rom[DESTINATION_OFFSET] = DestinationCode::Japanese.code();
        rom[0x4000] = 0x01;
        rom[2 * 0x4000] = 0x02;
        rom[257 * 0x4000] = 0x57;
        rom[HEADER_CHECKSUM_OFFSET] =
            compute_header_checksum(&rom).expect("checksum should compute");

        let mut cartridge = Cartridge::from_rom(rom).expect("mbc5 should be supported");
        assert_eq!(cartridge.read(0x4000), 0x01);

        cartridge.write(0x2000, 0x02);
        assert_eq!(cartridge.read(0x4000), 0x02);

        cartridge.write(0x2000, 0x01);
        cartridge.write(0x3000, 0x01);
        assert_eq!(cartridge.read(0x4000), 0x57);
    }

    #[test]
    fn mbc5_allows_selecting_rom_bank_zero_in_switchable_window() {
        let mut rom = vec![0u8; 4 * 0x4000];
        rom[CARTRIDGE_TYPE_OFFSET] = CartridgeType::Mbc5.code();
        rom[ROM_SIZE_OFFSET] = RomSize::Banks4.code();
        rom[RAM_SIZE_OFFSET] = RamSize::None.code();
        rom[DESTINATION_OFFSET] = DestinationCode::Japanese.code();
        rom[0x4000] = 0x11;
        rom[HEADER_CHECKSUM_OFFSET] =
            compute_header_checksum(&rom).expect("checksum should compute");

        let mut cartridge = Cartridge::from_rom(rom).expect("mbc5 should be supported");
        assert_eq!(cartridge.read(0x4000), 0x11);

        cartridge.write(0x2000, 0x00);
        cartridge.write(0x3000, 0x00);
        assert_eq!(cartridge.read(0x4000), 0x00);
    }

    #[test]
    fn mbc5_supports_ram_enable_and_bank_switching() {
        let mut rom = vec![0u8; 0x10000];
        rom[CARTRIDGE_TYPE_OFFSET] = CartridgeType::Mbc5Ram.code();
        rom[ROM_SIZE_OFFSET] = RomSize::Banks4.code();
        rom[RAM_SIZE_OFFSET] = RamSize::KibiBytes32.code();
        rom[DESTINATION_OFFSET] = DestinationCode::Japanese.code();
        rom[HEADER_CHECKSUM_OFFSET] =
            compute_header_checksum(&rom).expect("checksum should compute");

        let mut cartridge = Cartridge::from_rom(rom).expect("mbc5 should be supported");
        cartridge.write(0xA000, 0x12);
        assert_eq!(cartridge.read(0xA000), 0xFF);

        cartridge.write(0x0000, 0x0A);
        cartridge.write(0xA000, 0x12);
        assert_eq!(cartridge.read(0xA000), 0x12);

        cartridge.write(0x4000, 0x01);
        cartridge.write(0xA000, 0x34);
        assert_eq!(cartridge.read(0xA000), 0x34);

        cartridge.write(0x4000, 0x00);
        assert_eq!(cartridge.read(0xA000), 0x12);
    }

    #[test]
    fn cartridge_rejects_roms_shorter_than_header_declared_rom_size() {
        let mut rom = vec![0u8; 0x8000];
        rom[CARTRIDGE_TYPE_OFFSET] = CartridgeType::RomOnly.code();
        rom[ROM_SIZE_OFFSET] = RomSize::Banks4.code();
        rom[RAM_SIZE_OFFSET] = RamSize::None.code();
        rom[DESTINATION_OFFSET] = DestinationCode::Japanese.code();
        rom[HEADER_CHECKSUM_OFFSET] =
            compute_header_checksum(&rom).expect("checksum should compute");

        let error = Cartridge::from_rom(rom).expect_err("short rom should be rejected");
        assert_eq!(
            error,
            CartridgeError::RomSizeMismatch {
                expected_size: 0x10000,
                actual_size: 0x8000,
            }
        );
    }

    #[test]
    fn cartridge_save_data_round_trips_battery_backed_external_ram() {
        let mut rom = vec![0u8; 0x8000];
        rom[CARTRIDGE_TYPE_OFFSET] = CartridgeType::RomRamBattery.code();
        rom[ROM_SIZE_OFFSET] = RomSize::Banks2.code();
        rom[RAM_SIZE_OFFSET] = RamSize::KibiBytes8.code();
        rom[DESTINATION_OFFSET] = DestinationCode::Japanese.code();
        rom[HEADER_CHECKSUM_OFFSET] =
            compute_header_checksum(&rom).expect("checksum should compute");

        let mut cartridge = Cartridge::from_rom(rom).expect("rom+ram+battery cartridge loads");
        cartridge.write(0xA000, 0xAB);
        cartridge.write(0xA001, 0xCD);

        let save_data = cartridge
            .save_data()
            .expect("battery-backed cartridge should expose save data");
        assert_eq!(save_data[0], 0xAB);
        assert_eq!(save_data[1], 0xCD);

        let mut reloaded_cartridge =
            Cartridge::from_rom(cartridge.rom.clone()).expect("rom reload should succeed");
        reloaded_cartridge
            .load_save_data(&save_data)
            .expect("save data should load");

        assert_eq!(reloaded_cartridge.read(0xA000), 0xAB);
        assert_eq!(reloaded_cartridge.read(0xA001), 0xCD);
    }

    #[test]
    fn cartridge_save_data_is_unavailable_for_non_battery_types() {
        let mut rom = vec![0u8; 0x8000];
        rom[CARTRIDGE_TYPE_OFFSET] = CartridgeType::RomRam.code();
        rom[ROM_SIZE_OFFSET] = RomSize::Banks2.code();
        rom[RAM_SIZE_OFFSET] = RamSize::KibiBytes8.code();
        rom[DESTINATION_OFFSET] = DestinationCode::Japanese.code();
        rom[HEADER_CHECKSUM_OFFSET] =
            compute_header_checksum(&rom).expect("checksum should compute");

        let cartridge = Cartridge::from_rom(rom).expect("rom+ram cartridge loads");
        assert_eq!(cartridge.save_data(), None);
    }

    #[test]
    fn cartridge_load_save_data_rejects_size_mismatches() {
        let mut rom = vec![0u8; 0x8000];
        rom[CARTRIDGE_TYPE_OFFSET] = CartridgeType::RomRamBattery.code();
        rom[ROM_SIZE_OFFSET] = RomSize::Banks2.code();
        rom[RAM_SIZE_OFFSET] = RamSize::KibiBytes8.code();
        rom[DESTINATION_OFFSET] = DestinationCode::Japanese.code();
        rom[HEADER_CHECKSUM_OFFSET] =
            compute_header_checksum(&rom).expect("checksum should compute");

        let mut cartridge = Cartridge::from_rom(rom).expect("rom+ram+battery cartridge loads");
        let error = cartridge
            .load_save_data(&[0u8; 16])
            .expect_err("short save should be rejected");

        assert_eq!(
            error,
            SaveDataError::SizeMismatch {
                expected_size: 8 * 1024,
                actual_size: 16,
            }
        );
    }
}
