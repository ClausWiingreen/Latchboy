use latchboy_core::cartridge::{
    compute_header_checksum, Cartridge, CartridgeType, DestinationCode, RamSize, RomSize,
};

const ROM_SIZE_BYTES: usize = 2 * 16 * 1024;

pub fn cartridge_with_program(program: &[(usize, u8)], title: &[u8; 4]) -> Cartridge {
    let mut rom = vec![0u8; ROM_SIZE_BYTES];
    for (offset, value) in program {
        rom[*offset] = *value;
    }

    rom[0x0134..0x0138].copy_from_slice(title);
    rom[0x0147] = CartridgeType::RomOnly.code();
    rom[0x0148] = RomSize::Banks2.code();
    rom[0x0149] = RamSize::None.code();
    rom[0x014A] = DestinationCode::Japanese.code();
    rom[0x014D] = compute_header_checksum(&rom).expect("benchmark header checksum should compute");

    Cartridge::from_rom(rom).expect("benchmark ROM should parse")
}
