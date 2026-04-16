use crate::cartridge::Cartridge;

const VRAM_START: u16 = 0x8000;
const EXTERNAL_RAM_START: u16 = 0xA000;
const WRAM_START: u16 = 0xC000;
const WRAM_END: u16 = 0xDFFF;
const WRAM_ECHO_START: u16 = 0xE000;
const WRAM_ECHO_END: u16 = 0xFDFF;
const OAM_START: u16 = 0xFE00;
const OAM_END: u16 = 0xFE9F;
const UNUSABLE_START: u16 = 0xFEA0;
const UNUSABLE_END: u16 = 0xFEFF;
const IO_REGISTERS_START: u16 = 0xFF00;
const IO_REGISTERS_END: u16 = 0xFF7F;
const BOOT_ROM_DISABLE_REGISTER: u16 = 0xFF50;
const HRAM_START: u16 = 0xFF80;
const HRAM_END: u16 = 0xFFFE;
const INTERRUPT_ENABLE_REGISTER: u16 = 0xFFFF;

const VRAM_SIZE: usize = 0x2000;
const WRAM_SIZE: usize = 0x2000;
const OAM_SIZE: usize = 0xA0;
const IO_REGISTERS_SIZE: usize = 0x80;
const HRAM_SIZE: usize = 0x7F;
const BOOT_ROM_SIZE: usize = 0x100;
const NO_BOOT_DEFAULTS: &[(u16, u8)] = &[
    (0xFF05, 0x00),
    (0xFF06, 0x00),
    (0xFF07, 0x00),
    (0xFF10, 0x80),
    (0xFF11, 0xBF),
    (0xFF12, 0xF3),
    (0xFF14, 0xBF),
    (0xFF16, 0x3F),
    (0xFF17, 0x00),
    (0xFF19, 0xBF),
    (0xFF1A, 0x7F),
    (0xFF1B, 0xFF),
    (0xFF1C, 0x9F),
    (0xFF1E, 0xBF),
    (0xFF20, 0xFF),
    (0xFF21, 0x00),
    (0xFF22, 0x00),
    (0xFF23, 0xBF),
    (0xFF24, 0x77),
    (0xFF25, 0xF3),
    (0xFF26, 0xF1),
    (0xFF40, 0x91),
    (0xFF42, 0x00),
    (0xFF43, 0x00),
    (0xFF45, 0x00),
    (0xFF47, 0xFC),
    (0xFF48, 0xFF),
    (0xFF49, 0xFF),
    (0xFF4A, 0x00),
    (0xFF4B, 0x00),
    (0xFF50, 0x01),
    (0xFFFF, 0x00),
];

/// DMG address bus with full address-range mapping and WRAM echo behavior.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Bus {
    cartridge: Cartridge,
    boot_rom: Option<Vec<u8>>,
    boot_rom_enabled: bool,
    boot_rom_disable_value: u8,
    vram: [u8; VRAM_SIZE],
    wram: [u8; WRAM_SIZE],
    oam: [u8; OAM_SIZE],
    io_registers: [u8; IO_REGISTERS_SIZE],
    hram: [u8; HRAM_SIZE],
    interrupt_enable: u8,
}

impl Bus {
    pub fn new(cartridge: Cartridge) -> Self {
        Self {
            cartridge,
            boot_rom: None,
            boot_rom_enabled: false,
            boot_rom_disable_value: 0,
            vram: [0; VRAM_SIZE],
            wram: [0; WRAM_SIZE],
            oam: [0; OAM_SIZE],
            io_registers: [0; IO_REGISTERS_SIZE],
            hram: [0; HRAM_SIZE],
            interrupt_enable: 0,
        }
    }

    pub fn with_boot_rom(cartridge: Cartridge, boot_rom: Vec<u8>) -> Self {
        let mut bus = Self::new(cartridge);
        bus.boot_rom = Some(boot_rom);
        bus.boot_rom_enabled = true;
        bus
    }

    pub const fn boot_rom_enabled(&self) -> bool {
        self.boot_rom_enabled
    }

    pub fn read8(&self, address: u16) -> u8 {
        match address {
            0x0000..=0x7FFF => {
                if self.boot_rom_enabled && address < BOOT_ROM_SIZE as u16 {
                    return self
                        .boot_rom
                        .as_ref()
                        .and_then(|rom| rom.get(address as usize))
                        .copied()
                        .unwrap_or(0xFF);
                }

                self.cartridge.read(address)
            }
            VRAM_START..=0x9FFF => self.vram[(address - VRAM_START) as usize],
            EXTERNAL_RAM_START..=0xBFFF => self.cartridge.read(address),
            WRAM_START..=WRAM_END => self.wram[(address - WRAM_START) as usize],
            WRAM_ECHO_START..=WRAM_ECHO_END => self.wram[(address - WRAM_ECHO_START) as usize],
            OAM_START..=OAM_END => self.oam[(address - OAM_START) as usize],
            UNUSABLE_START..=UNUSABLE_END => 0xFF,
            IO_REGISTERS_START..=IO_REGISTERS_END => {
                if address == BOOT_ROM_DISABLE_REGISTER {
                    self.boot_rom_disable_value
                } else {
                    self.io_registers[(address - IO_REGISTERS_START) as usize]
                }
            }
            HRAM_START..=HRAM_END => self.hram[(address - HRAM_START) as usize],
            INTERRUPT_ENABLE_REGISTER => self.interrupt_enable,
        }
    }

    pub fn reset(&mut self) {
        self.cartridge.reset_mapper_state();

        self.boot_rom_enabled = self.boot_rom.is_some();
        self.boot_rom_disable_value = 0;
        self.vram = [0; VRAM_SIZE];
        self.wram = [0; WRAM_SIZE];
        self.oam = [0; OAM_SIZE];
        self.io_registers = [0; IO_REGISTERS_SIZE];
        self.hram = [0; HRAM_SIZE];
        self.interrupt_enable = 0;
    }

    pub fn apply_dmg_no_boot_defaults(&mut self) {
        for (address, value) in NO_BOOT_DEFAULTS {
            self.write8(*address, *value);
        }
    }

    pub fn write8(&mut self, address: u16, value: u8) {
        match address {
            0x0000..=0x7FFF => self.cartridge.write(address, value),
            VRAM_START..=0x9FFF => self.vram[(address - VRAM_START) as usize] = value,
            EXTERNAL_RAM_START..=0xBFFF => self.cartridge.write(address, value),
            WRAM_START..=WRAM_END => self.wram[(address - WRAM_START) as usize] = value,
            WRAM_ECHO_START..=WRAM_ECHO_END => {
                self.wram[(address - WRAM_ECHO_START) as usize] = value
            }
            OAM_START..=OAM_END => self.oam[(address - OAM_START) as usize] = value,
            UNUSABLE_START..=UNUSABLE_END => {}
            IO_REGISTERS_START..=IO_REGISTERS_END => {
                if address == BOOT_ROM_DISABLE_REGISTER {
                    self.boot_rom_disable_value = value;
                    if self.boot_rom_enabled && value != 0 {
                        self.boot_rom_enabled = false;
                    }
                } else {
                    self.io_registers[(address - IO_REGISTERS_START) as usize] = value;
                }
            }
            HRAM_START..=HRAM_END => self.hram[(address - HRAM_START) as usize] = value,
            INTERRUPT_ENABLE_REGISTER => self.interrupt_enable = value,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::cartridge::{
        compute_header_checksum, Cartridge, CartridgeType, DestinationCode, RamSize, RomSize,
    };

    use super::*;

    fn make_rom(cartridge_type: CartridgeType, ram_size: RamSize) -> Vec<u8> {
        let mut rom = vec![0u8; 2 * 16 * 1024];
        rom[0x0134..0x0138].copy_from_slice(b"TEST");
        rom[0x0147] = cartridge_type.code();
        rom[0x0148] = RomSize::Banks2.code();
        rom[0x0149] = ram_size.code();
        rom[0x014A] = DestinationCode::Japanese.code();
        rom[0x014D] = compute_header_checksum(&rom).expect("header checksum should compute");
        rom
    }

    fn make_cartridge(cartridge_type: CartridgeType, ram_size: RamSize) -> Cartridge {
        Cartridge::from_rom(make_rom(cartridge_type, ram_size)).expect("test rom should parse")
    }

    #[test]
    fn bus_routes_reads_and_writes_across_internal_ranges() {
        let cartridge = make_cartridge(CartridgeType::RomOnly, RamSize::None);
        let mut bus = Bus::new(cartridge);

        bus.write8(0x8000, 0x12);
        bus.write8(0xC000, 0x34);
        bus.write8(0xFE00, 0x56);
        bus.write8(0xFF10, 0x78);
        bus.write8(0xFF80, 0x9A);
        bus.write8(0xFFFF, 0xBC);

        assert_eq!(bus.read8(0x8000), 0x12);
        assert_eq!(bus.read8(0xC000), 0x34);
        assert_eq!(bus.read8(0xFE00), 0x56);
        assert_eq!(bus.read8(0xFF10), 0x78);
        assert_eq!(bus.read8(0xFF80), 0x9A);
        assert_eq!(bus.read8(0xFFFF), 0xBC);
    }

    #[test]
    fn wram_echo_is_bidirectionally_mirrored() {
        let cartridge = make_cartridge(CartridgeType::RomOnly, RamSize::None);
        let mut bus = Bus::new(cartridge);

        bus.write8(0xC123, 0x5A);
        assert_eq!(bus.read8(0xE123), 0x5A);

        bus.write8(0xEABC, 0xA5);
        assert_eq!(bus.read8(0xCABC), 0xA5);
    }

    #[test]
    fn unusable_region_returns_ff_and_ignores_writes() {
        let cartridge = make_cartridge(CartridgeType::RomOnly, RamSize::None);
        let mut bus = Bus::new(cartridge);

        bus.write8(0xFEA0, 0x11);
        assert_eq!(bus.read8(0xFEA0), 0xFF);
    }

    #[test]
    fn ff50_disables_boot_rom_when_written_non_zero() {
        let mut cartridge_rom = make_rom(CartridgeType::RomOnly, RamSize::None);
        cartridge_rom[0] = 0x99;
        let cartridge = Cartridge::from_rom(cartridge_rom).expect("test rom should parse");

        let mut bus = Bus::with_boot_rom(cartridge, vec![0x42; BOOT_ROM_SIZE]);
        assert_eq!(bus.read8(0x0000), 0x42);

        bus.write8(0xFF50, 0x01);
        assert!(!bus.boot_rom_enabled());
        assert_eq!(bus.read8(0xFF50), 0x01);
        assert_eq!(bus.read8(0x0000), 0x99);

        bus.write8(0xFF50, 0x00);
        assert!(!bus.boot_rom_enabled());
    }

    #[test]
    fn reset_reenables_boot_rom_mapping_when_boot_rom_is_present() {
        let mut cartridge_rom = make_rom(CartridgeType::RomOnly, RamSize::None);
        cartridge_rom[0] = 0x99;
        let cartridge = Cartridge::from_rom(cartridge_rom).expect("test rom should parse");

        let mut bus = Bus::with_boot_rom(cartridge, vec![0x42; BOOT_ROM_SIZE]);
        bus.write8(0xFF50, 0x01);
        assert!(!bus.boot_rom_enabled());

        bus.reset();

        assert!(bus.boot_rom_enabled());
        assert_eq!(bus.read8(0xFF50), 0x00);
        assert_eq!(bus.read8(0x0000), 0x42);
    }

    #[test]
    fn reset_restores_mapper_default_bank_selection() {
        let mut rom = vec![0u8; 4 * 16 * 1024];
        rom[0x4000] = 0x11; // bank 1
        rom[0x8000] = 0x22; // bank 2
        rom[0x0134..0x0138].copy_from_slice(b"BANK");
        rom[0x0147] = CartridgeType::Mbc1.code();
        rom[0x0148] = RomSize::Banks4.code();
        rom[0x0149] = RamSize::None.code();
        rom[0x014A] = DestinationCode::Japanese.code();
        rom[0x014D] = compute_header_checksum(&rom).expect("header checksum should compute");

        let cartridge = Cartridge::from_rom(rom).expect("test rom should parse");
        let mut bus = Bus::new(cartridge);

        assert_eq!(bus.read8(0x4000), 0x11);
        bus.write8(0x2000, 0x02);
        assert_eq!(bus.read8(0x4000), 0x22);

        bus.reset();

        assert_eq!(bus.read8(0x4000), 0x11);
    }

    #[test]
    fn cartridge_ram_access_is_routed_through_bus() {
        let cartridge = make_cartridge(CartridgeType::Mbc1RamBattery, RamSize::KibiBytes32);
        let mut bus = Bus::new(cartridge);

        bus.write8(0x0000, 0x0A);
        bus.write8(0xA000, 0x77);

        assert_eq!(bus.read8(0xA000), 0x77);
    }
}
