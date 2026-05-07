use num_enum::{IntoPrimitive, TryFromPrimitive};

use super::CartridgeError;

pub const CARTRIDGE_HEADER_SIZE: usize = 0x150;
pub const TITLE_START: usize = 0x0134;
pub const TITLE_END_INCLUSIVE: usize = 0x0142;
pub const CGB_FLAG_OFFSET: usize = 0x0143;
pub const HEADER_CHECKSUM_START: usize = 0x0134;
pub const HEADER_CHECKSUM_END_INCLUSIVE: usize = 0x014C;
pub const CARTRIDGE_TYPE_OFFSET: usize = 0x0147;
pub const ROM_SIZE_OFFSET: usize = 0x0148;
pub const RAM_SIZE_OFFSET: usize = 0x0149;
pub const DESTINATION_OFFSET: usize = 0x014A;
pub const HEADER_CHECKSUM_OFFSET: usize = 0x014D;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum HeaderWarning {
    HeaderChecksumMismatch { expected: u8, actual: u8 },
    UnknownCartridgeType(u8),
    UnknownRomSizeCode(u8),
    UnknownRamSizeCode(u8),
    UnknownDestinationCode(u8),
    UnknownCgbFlag(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, IntoPrimitive, TryFromPrimitive)]
#[repr(u8)]
enum KnownCartridgeType {
    RomOnly = 0x00,
    RomRam = 0x08,
    RomRamBattery = 0x09,
    Mbc1 = 0x01,
    Mbc1Ram = 0x02,
    Mbc1RamBattery = 0x03,
    Mbc3TimerBattery = 0x0F,
    Mbc3TimerRamBattery = 0x10,
    Mbc3 = 0x11,
    Mbc3Ram = 0x12,
    Mbc3RamBattery = 0x13,
    Mbc5 = 0x19,
    Mbc5Ram = 0x1A,
    Mbc5RamBattery = 0x1B,
    Mbc5Rumble = 0x1C,
    Mbc5RumbleRam = 0x1D,
    Mbc5RumbleRamBattery = 0x1E,
}

impl From<KnownCartridgeType> for CartridgeType {
    fn from(value: KnownCartridgeType) -> Self {
        match value {
            KnownCartridgeType::RomOnly => Self::RomOnly,
            KnownCartridgeType::RomRam => Self::RomRam,
            KnownCartridgeType::RomRamBattery => Self::RomRamBattery,
            KnownCartridgeType::Mbc1 => Self::Mbc1,
            KnownCartridgeType::Mbc1Ram => Self::Mbc1Ram,
            KnownCartridgeType::Mbc1RamBattery => Self::Mbc1RamBattery,
            KnownCartridgeType::Mbc3TimerBattery => Self::Mbc3TimerBattery,
            KnownCartridgeType::Mbc3TimerRamBattery => Self::Mbc3TimerRamBattery,
            KnownCartridgeType::Mbc3 => Self::Mbc3,
            KnownCartridgeType::Mbc3Ram => Self::Mbc3Ram,
            KnownCartridgeType::Mbc3RamBattery => Self::Mbc3RamBattery,
            KnownCartridgeType::Mbc5 => Self::Mbc5,
            KnownCartridgeType::Mbc5Ram => Self::Mbc5Ram,
            KnownCartridgeType::Mbc5RamBattery => Self::Mbc5RamBattery,
            KnownCartridgeType::Mbc5Rumble => Self::Mbc5Rumble,
            KnownCartridgeType::Mbc5RumbleRam => Self::Mbc5RumbleRam,
            KnownCartridgeType::Mbc5RumbleRamBattery => Self::Mbc5RumbleRamBattery,
        }
    }
}

impl CartridgeType {
    pub fn from_code(value: u8) -> Self {
        KnownCartridgeType::try_from(value)
            .map(Self::from)
            .unwrap_or(Self::Unknown(value))
    }

    pub const fn code(self) -> u8 {
        match self {
            Self::RomOnly => KnownCartridgeType::RomOnly as u8,
            Self::RomRam => KnownCartridgeType::RomRam as u8,
            Self::RomRamBattery => KnownCartridgeType::RomRamBattery as u8,
            Self::Mbc1 => KnownCartridgeType::Mbc1 as u8,
            Self::Mbc1Ram => KnownCartridgeType::Mbc1Ram as u8,
            Self::Mbc1RamBattery => KnownCartridgeType::Mbc1RamBattery as u8,
            Self::Mbc3TimerBattery => KnownCartridgeType::Mbc3TimerBattery as u8,
            Self::Mbc3TimerRamBattery => KnownCartridgeType::Mbc3TimerRamBattery as u8,
            Self::Mbc3 => KnownCartridgeType::Mbc3 as u8,
            Self::Mbc3Ram => KnownCartridgeType::Mbc3Ram as u8,
            Self::Mbc3RamBattery => KnownCartridgeType::Mbc3RamBattery as u8,
            Self::Mbc5 => KnownCartridgeType::Mbc5 as u8,
            Self::Mbc5Ram => KnownCartridgeType::Mbc5Ram as u8,
            Self::Mbc5RamBattery => KnownCartridgeType::Mbc5RamBattery as u8,
            Self::Mbc5Rumble => KnownCartridgeType::Mbc5Rumble as u8,
            Self::Mbc5RumbleRam => KnownCartridgeType::Mbc5RumbleRam as u8,
            Self::Mbc5RumbleRamBattery => KnownCartridgeType::Mbc5RumbleRamBattery as u8,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, IntoPrimitive, TryFromPrimitive)]
#[repr(u8)]
enum KnownRomSize {
    Banks2 = 0x00,
    Banks4 = 0x01,
    Banks8 = 0x02,
    Banks16 = 0x03,
    Banks32 = 0x04,
    Banks64 = 0x05,
    Banks128 = 0x06,
    Banks256 = 0x07,
    Banks512 = 0x08,
    Banks72 = 0x52,
    Banks80 = 0x53,
    Banks96 = 0x54,
}

impl From<KnownRomSize> for RomSize {
    fn from(value: KnownRomSize) -> Self {
        match value {
            KnownRomSize::Banks2 => Self::Banks2,
            KnownRomSize::Banks4 => Self::Banks4,
            KnownRomSize::Banks8 => Self::Banks8,
            KnownRomSize::Banks16 => Self::Banks16,
            KnownRomSize::Banks32 => Self::Banks32,
            KnownRomSize::Banks64 => Self::Banks64,
            KnownRomSize::Banks128 => Self::Banks128,
            KnownRomSize::Banks256 => Self::Banks256,
            KnownRomSize::Banks512 => Self::Banks512,
            KnownRomSize::Banks72 => Self::Banks72,
            KnownRomSize::Banks80 => Self::Banks80,
            KnownRomSize::Banks96 => Self::Banks96,
        }
    }
}

impl RomSize {
    pub fn from_code(value: u8) -> Self {
        KnownRomSize::try_from(value)
            .map(Self::from)
            .unwrap_or(Self::Unknown(value))
    }
    pub const fn code(self) -> u8 {
        match self {
            Self::Banks2 => KnownRomSize::Banks2 as u8,
            Self::Banks4 => KnownRomSize::Banks4 as u8,
            Self::Banks8 => KnownRomSize::Banks8 as u8,
            Self::Banks16 => KnownRomSize::Banks16 as u8,
            Self::Banks32 => KnownRomSize::Banks32 as u8,
            Self::Banks64 => KnownRomSize::Banks64 as u8,
            Self::Banks128 => KnownRomSize::Banks128 as u8,
            Self::Banks256 => KnownRomSize::Banks256 as u8,
            Self::Banks512 => KnownRomSize::Banks512 as u8,
            Self::Banks72 => KnownRomSize::Banks72 as u8,
            Self::Banks80 => KnownRomSize::Banks80 as u8,
            Self::Banks96 => KnownRomSize::Banks96 as u8,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RamSize {
    None,
    KibiBytes8,
    KibiBytes32,
    KibiBytes64,
    KibiBytes128,
    Unknown(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, IntoPrimitive, TryFromPrimitive)]
#[repr(u8)]
enum KnownRamSize {
    None = 0x00,
    KibiBytes8 = 0x02,
    KibiBytes32 = 0x03,
    KibiBytes128 = 0x04,
    KibiBytes64 = 0x05,
}

impl From<KnownRamSize> for RamSize {
    fn from(value: KnownRamSize) -> Self {
        match value {
            KnownRamSize::None => Self::None,
            KnownRamSize::KibiBytes8 => Self::KibiBytes8,
            KnownRamSize::KibiBytes32 => Self::KibiBytes32,
            KnownRamSize::KibiBytes64 => Self::KibiBytes64,
            KnownRamSize::KibiBytes128 => Self::KibiBytes128,
        }
    }
}

impl RamSize {
    pub fn from_code(value: u8) -> Self {
        KnownRamSize::try_from(value)
            .map(Self::from)
            .unwrap_or(Self::Unknown(value))
    }
    pub const fn code(self) -> u8 {
        match self {
            Self::None => KnownRamSize::None as u8,
            Self::KibiBytes8 => KnownRamSize::KibiBytes8 as u8,
            Self::KibiBytes32 => KnownRamSize::KibiBytes32 as u8,
            Self::KibiBytes128 => KnownRamSize::KibiBytes128 as u8,
            Self::KibiBytes64 => KnownRamSize::KibiBytes64 as u8,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CgbCompatibility {
    DmgOnly,
    CgbEnhanced,
    CgbOnly,
    Unknown(u8),
}

impl CgbCompatibility {
    pub const fn from_flag(value: u8) -> Self {
        match value {
            0x80 => Self::CgbEnhanced,
            0xC0 => Self::CgbOnly,
            other if (other & 0x80) == 0 => Self::DmgOnly,
            other => Self::Unknown(other),
        }
    }

    pub const fn flag(self) -> u8 {
        match self {
            Self::DmgOnly => 0x00,
            Self::CgbEnhanced => 0x80,
            Self::CgbOnly => 0xC0,
            Self::Unknown(value) => value,
        }
    }

    pub const fn supports_cgb(self) -> bool {
        matches!(self, Self::CgbEnhanced | Self::CgbOnly)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DestinationCode {
    Japanese,
    NonJapanese,
    Unknown(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, IntoPrimitive, TryFromPrimitive)]
#[repr(u8)]
enum KnownDestinationCode {
    Japanese = 0x00,
    NonJapanese = 0x01,
}

impl From<KnownDestinationCode> for DestinationCode {
    fn from(value: KnownDestinationCode) -> Self {
        match value {
            KnownDestinationCode::Japanese => Self::Japanese,
            KnownDestinationCode::NonJapanese => Self::NonJapanese,
        }
    }
}

impl DestinationCode {
    pub fn from_code(value: u8) -> Self {
        KnownDestinationCode::try_from(value)
            .map(Self::from)
            .unwrap_or(Self::Unknown(value))
    }
    pub const fn code(self) -> u8 {
        match self {
            Self::Japanese => KnownDestinationCode::Japanese as u8,
            Self::NonJapanese => KnownDestinationCode::NonJapanese as u8,
            Self::Unknown(value) => value,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CartridgeHeader {
    pub title: String,
    pub cartridge_type: CartridgeType,
    pub rom_size: RomSize,
    pub ram_size: RamSize,
    pub cgb_compatibility: CgbCompatibility,
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
            cgb_compatibility: CgbCompatibility::from_flag(rom[CGB_FLAG_OFFSET]),
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
        if let CgbCompatibility::Unknown(flag) = self.cgb_compatibility {
            warnings.push(HeaderWarning::UnknownCgbFlag(flag));
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
