pub mod header;
mod mapper;
pub mod save_data;

pub use header::{
    compute_header_checksum, CartridgeHeader, CartridgeType, CgbCompatibility, DestinationCode,
    HeaderWarning, RamSize, RomSize,
};
pub use save_data::SaveDataError;

#[cfg(test)]
use header::{
    CARTRIDGE_HEADER_SIZE, CARTRIDGE_TYPE_OFFSET, CGB_FLAG_OFFSET, DESTINATION_OFFSET,
    HEADER_CHECKSUM_OFFSET, RAM_SIZE_OFFSET, ROM_SIZE_OFFSET, TITLE_END_INCLUSIVE, TITLE_START,
};
use mapper::Mapper;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Cartridge {
    pub header: CartridgeHeader,
    pub warnings: Vec<HeaderWarning>,
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
        let warnings = header.warnings();
        let external_ram = header.ram_size.to_bytes().map(|size| vec![0u8; size]);
        let mapper = Mapper::new(header.cartridge_type, rom, external_ram)?;
        Ok(Self {
            header,
            warnings,
            mapper,
        })
    }

    pub fn read8(&self, address: u16) -> u8 {
        match address {
            0x0000..=0x7FFF => self.mapper.read_rom(address),
            0xA000..=0xBFFF => self.mapper.read_ram(address),
            _ => 0xFF,
        }
    }

    pub fn write8(&mut self, address: u16, value: u8) {
        match address {
            0x0000..=0x7FFF => self.mapper.write_control(address, value),
            0xA000..=0xBFFF => self.mapper.write_ram(address, value),
            _ => {}
        }
    }

    pub fn reset(&mut self) {
        self.mapper.reset();
    }

    pub const fn has_battery_backed_ram(&self) -> bool {
        self.header.cartridge_type.has_battery_backed_ram()
    }

    pub fn save_data(&self) -> Option<Vec<u8>> {
        if !self.has_battery_backed_ram() {
            return None;
        }
        self.mapper.save_data()
    }

    pub fn load_save_data(&mut self, save_data: &[u8]) -> Result<(), SaveDataError> {
        if !self.has_battery_backed_ram() {
            return Err(SaveDataError::NotBatteryBackedRam);
        }
        self.mapper.load_save_data(save_data)
    }

    #[cfg(test)]
    fn rom(&self) -> &[u8] {
        self.mapper.rom()
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
        assert_eq!(header.cgb_compatibility, CgbCompatibility::DmgOnly);
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
        rom[CGB_FLAG_OFFSET] = 0x40;
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
        assert!(warnings
            .iter()
            .any(|warning| matches!(warning, HeaderWarning::UnknownCgbFlag(0x40))));
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
        rom[CGB_FLAG_OFFSET] = 0x80;
        rom[HEADER_CHECKSUM_OFFSET] =
            compute_header_checksum(&rom).expect("checksum should compute");

        let header = CartridgeHeader::parse(&rom).expect("header should parse");

        assert_eq!(header.title, "AAAAAAAAAAAAAAA");
        assert_eq!(header.cgb_compatibility, CgbCompatibility::CgbEnhanced);
    }

    #[test]
    fn parse_distinguishes_cgb_enhanced_and_cgb_only_flags() {
        let mut enhanced_rom = make_test_rom();
        enhanced_rom[CGB_FLAG_OFFSET] = CgbCompatibility::CgbEnhanced.flag();
        enhanced_rom[HEADER_CHECKSUM_OFFSET] =
            compute_header_checksum(&enhanced_rom).expect("checksum should compute");
        let enhanced = CartridgeHeader::parse(&enhanced_rom).expect("header should parse");
        assert_eq!(enhanced.cgb_compatibility, CgbCompatibility::CgbEnhanced);
        assert!(enhanced.cgb_compatibility.supports_cgb());

        let mut cgb_only_rom = make_test_rom();
        cgb_only_rom[CGB_FLAG_OFFSET] = CgbCompatibility::CgbOnly.flag();
        cgb_only_rom[HEADER_CHECKSUM_OFFSET] =
            compute_header_checksum(&cgb_only_rom).expect("checksum should compute");
        let cgb_only = CartridgeHeader::parse(&cgb_only_rom).expect("header should parse");
        assert_eq!(cgb_only.cgb_compatibility, CgbCompatibility::CgbOnly);
        assert!(cgb_only.cgb_compatibility.supports_cgb());
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

        assert_eq!(cartridge.read8(0x1234), 0x42);
        cartridge.write8(0x1234, 0x99);
        assert_eq!(cartridge.read8(0x1234), 0x42);
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
        assert_eq!(cartridge.read8(0xA123), 0x00);

        cartridge.write8(0xA123, 0x77);
        assert_eq!(cartridge.read8(0xA123), 0x77);
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
        assert_eq!(cartridge.read8(0x1000), 0x00);
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

        assert_eq!(cartridge.read8(0x4000), 0x01);
        cartridge.write8(0x2000, 0x02);
        assert_eq!(cartridge.read8(0x4000), 0x02);
        cartridge.write8(0x2000, 0x03);
        assert_eq!(cartridge.read8(0x4000), 0x03);
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

        cartridge.write8(0xA000, 0x55);
        assert_eq!(cartridge.read8(0xA000), 0xFF);

        cartridge.write8(0x0000, 0x0A);
        cartridge.write8(0xA000, 0x55);
        assert_eq!(cartridge.read8(0xA000), 0x55);

        cartridge.write8(0x0000, 0x00);
        assert_eq!(cartridge.read8(0xA000), 0xFF);
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
        cartridge.write8(0x2000, 0x01);
        cartridge.write8(0x4000, 0x01);
        cartridge.write8(0x6000, 0x01);

        assert_eq!(cartridge.read8(0x0000), 0x20);
        assert_eq!(cartridge.read8(0x4000), 0x21);
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
        cartridge.write8(0x0000, 0x0A);
        cartridge.write8(0x6000, 0x01);
        cartridge.write8(0x4000, 0x03);
        cartridge.write8(0xA000, 0x66);

        assert_eq!(cartridge.read8(0xA000), 0x66);
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
        assert_eq!(cartridge.read8(0x1000), 0x00);
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
        assert_eq!(cartridge.read8(0x4000), 0x01);

        cartridge.write8(0x2000, 0x02);
        assert_eq!(cartridge.read8(0x4000), 0x02);

        cartridge.write8(0x2000, 0x03);
        assert_eq!(cartridge.read8(0x4000), 0x03);
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

        cartridge.write8(0xA000, 0x12);
        assert_eq!(cartridge.read8(0xA000), 0xFF);

        cartridge.write8(0x0000, 0x0A);
        cartridge.write8(0xA000, 0x12);
        assert_eq!(cartridge.read8(0xA000), 0x12);

        cartridge.write8(0x4000, 0x01);
        cartridge.write8(0xA000, 0x34);
        assert_eq!(cartridge.read8(0xA000), 0x34);

        cartridge.write8(0x4000, 0x81);
        cartridge.write8(0xA000, 0x56);
        assert_eq!(cartridge.read8(0xA000), 0x56);

        cartridge.write8(0x4000, 0x00);
        assert_eq!(cartridge.read8(0xA000), 0x12);

        cartridge.write8(0x4000, 0x08);
        assert_eq!(cartridge.read8(0xA000), 0xFF);
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
        assert_eq!(cartridge.read8(0x1000), 0x00);
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
        assert_eq!(cartridge.read8(0x4000), 0x01);

        cartridge.write8(0x2000, 0x02);
        assert_eq!(cartridge.read8(0x4000), 0x02);

        cartridge.write8(0x2000, 0x01);
        cartridge.write8(0x3000, 0x01);
        assert_eq!(cartridge.read8(0x4000), 0x57);
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
        assert_eq!(cartridge.read8(0x4000), 0x11);

        cartridge.write8(0x2000, 0x00);
        cartridge.write8(0x3000, 0x00);
        assert_eq!(cartridge.read8(0x4000), 0x00);
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
        cartridge.write8(0xA000, 0x12);
        assert_eq!(cartridge.read8(0xA000), 0xFF);

        cartridge.write8(0x0000, 0x0A);
        cartridge.write8(0xA000, 0x12);
        assert_eq!(cartridge.read8(0xA000), 0x12);

        cartridge.write8(0x4000, 0x01);
        cartridge.write8(0xA000, 0x34);
        assert_eq!(cartridge.read8(0xA000), 0x34);

        cartridge.write8(0x4000, 0x00);
        assert_eq!(cartridge.read8(0xA000), 0x12);
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
        cartridge.write8(0xA000, 0xAB);
        cartridge.write8(0xA001, 0xCD);

        let save_data = cartridge
            .save_data()
            .expect("battery-backed cartridge should expose save data");
        assert_eq!(save_data[0], 0xAB);
        assert_eq!(save_data[1], 0xCD);

        let mut reloaded_cartridge =
            Cartridge::from_rom(cartridge.rom().to_vec()).expect("rom reload should succeed");
        reloaded_cartridge
            .load_save_data(&save_data)
            .expect("save data should load");

        assert_eq!(reloaded_cartridge.read8(0xA000), 0xAB);
        assert_eq!(reloaded_cartridge.read8(0xA001), 0xCD);
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

    #[test]
    fn cartridge_load_save_data_rejects_non_battery_ram_types() {
        let mut rom = vec![0u8; 0x8000];
        rom[CARTRIDGE_TYPE_OFFSET] = CartridgeType::RomRam.code();
        rom[ROM_SIZE_OFFSET] = RomSize::Banks2.code();
        rom[RAM_SIZE_OFFSET] = RamSize::KibiBytes8.code();
        rom[DESTINATION_OFFSET] = DestinationCode::Japanese.code();
        rom[HEADER_CHECKSUM_OFFSET] =
            compute_header_checksum(&rom).expect("checksum should compute");

        let mut cartridge = Cartridge::from_rom(rom).expect("rom+ram cartridge loads");
        let error = cartridge
            .load_save_data(&[0u8; 8 * 1024])
            .expect_err("non-battery cartridges should reject loading save data");

        assert_eq!(error, SaveDataError::NotBatteryBackedRam);
    }

    #[test]
    fn cartridge_load_save_data_rejects_battery_types_without_external_ram() {
        let mut rom = vec![0u8; 0x8000];
        rom[CARTRIDGE_TYPE_OFFSET] = CartridgeType::RomRamBattery.code();
        rom[ROM_SIZE_OFFSET] = RomSize::Banks2.code();
        rom[RAM_SIZE_OFFSET] = RamSize::None.code();
        rom[DESTINATION_OFFSET] = DestinationCode::Japanese.code();
        rom[HEADER_CHECKSUM_OFFSET] =
            compute_header_checksum(&rom).expect("checksum should compute");

        let mut cartridge = Cartridge::from_rom(rom).expect("rom should load");
        let error = cartridge
            .load_save_data(&[0u8; 8])
            .expect_err("cartridge has no external ram");

        assert_eq!(error, SaveDataError::NoExternalRam);
    }

    #[test]
    fn mbc1_battery_backed_external_ram_round_trips() {
        let mut rom = vec![0u8; 0x10000];
        rom[CARTRIDGE_TYPE_OFFSET] = CartridgeType::Mbc1RamBattery.code();
        rom[ROM_SIZE_OFFSET] = RomSize::Banks4.code();
        rom[RAM_SIZE_OFFSET] = RamSize::KibiBytes32.code();
        rom[DESTINATION_OFFSET] = DestinationCode::Japanese.code();
        rom[HEADER_CHECKSUM_OFFSET] =
            compute_header_checksum(&rom).expect("checksum should compute");

        let mut cartridge = Cartridge::from_rom(rom).expect("mbc1 cartridge loads");
        cartridge.write8(0x0000, 0x0A);
        cartridge.write8(0x4000, 0x01);
        cartridge.write8(0x6000, 0x01);
        cartridge.write8(0xA010, 0x5A);

        let save = cartridge
            .save_data()
            .expect("battery-backed save available");
        let mut reloaded = Cartridge::from_rom(cartridge.rom().to_vec()).expect("rom reload works");
        reloaded
            .load_save_data(&save)
            .expect("save data should load into cartridge");
        reloaded.write8(0x0000, 0x0A);
        reloaded.write8(0x4000, 0x01);
        reloaded.write8(0x6000, 0x01);
        assert_eq!(reloaded.read8(0xA010), 0x5A);
    }

    #[test]
    fn mbc5_battery_backed_external_ram_round_trips() {
        let mut rom = vec![0u8; 0x10000];
        rom[CARTRIDGE_TYPE_OFFSET] = CartridgeType::Mbc5RamBattery.code();
        rom[ROM_SIZE_OFFSET] = RomSize::Banks4.code();
        rom[RAM_SIZE_OFFSET] = RamSize::KibiBytes32.code();
        rom[DESTINATION_OFFSET] = DestinationCode::Japanese.code();
        rom[HEADER_CHECKSUM_OFFSET] =
            compute_header_checksum(&rom).expect("checksum should compute");

        let mut cartridge = Cartridge::from_rom(rom).expect("mbc5 cartridge loads");
        cartridge.write8(0x0000, 0x0A);
        cartridge.write8(0x4000, 0x01);
        cartridge.write8(0xA010, 0xA5);

        let save = cartridge
            .save_data()
            .expect("battery-backed save available");
        let mut reloaded = Cartridge::from_rom(cartridge.rom().to_vec()).expect("rom reload works");
        reloaded
            .load_save_data(&save)
            .expect("save data should load into cartridge");
        reloaded.write8(0x0000, 0x0A);
        reloaded.write8(0x4000, 0x01);
        assert_eq!(reloaded.read8(0xA010), 0xA5);
    }
}
