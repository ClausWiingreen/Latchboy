const CARTRIDGE_HEADER_SIZE: usize = 0x150;
const TITLE_START: usize = 0x0134;
const TITLE_END_INCLUSIVE: usize = 0x0143;
const HEADER_CHECKSUM_START: usize = 0x0134;
const HEADER_CHECKSUM_END_INCLUSIVE: usize = 0x014C;
const CARTRIDGE_TYPE_OFFSET: usize = 0x0147;
const ROM_SIZE_OFFSET: usize = 0x0148;
const RAM_SIZE_OFFSET: usize = 0x0149;
const DESTINATION_OFFSET: usize = 0x014A;
const HEADER_CHECKSUM_OFFSET: usize = 0x014D;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CartridgeError {
    RomTooSmall { actual_size: usize },
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

        Ok(Self {
            title,
            cartridge_type: CartridgeType::from_code(rom[CARTRIDGE_TYPE_OFFSET]),
            rom_size: RomSize::from_code(rom[ROM_SIZE_OFFSET]),
            ram_size: RamSize::from_code(rom[RAM_SIZE_OFFSET]),
            destination_code: DestinationCode::from_code(rom[DESTINATION_OFFSET]),
            header_checksum,
            computed_header_checksum: compute_header_checksum(rom),
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

pub fn compute_header_checksum(rom: &[u8]) -> u8 {
    rom[HEADER_CHECKSUM_START..=HEADER_CHECKSUM_END_INCLUSIVE]
        .iter()
        .fold(0u8, |acc, byte| acc.wrapping_sub(*byte).wrapping_sub(1))
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

        let checksum = compute_header_checksum(&rom);
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
        rom[HEADER_CHECKSUM_OFFSET] = compute_header_checksum(&rom);

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
}
