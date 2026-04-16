use crate::cartridge::Cartridge;
use crate::ppu::Ppu;
use crate::timer::{Timer, DIV_REGISTER, TAC_REGISTER, TIMA_REGISTER, TMA_REGISTER};

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

const WRAM_SIZE: usize = 0x2000;
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
    ppu: Ppu,
    wram: [u8; WRAM_SIZE],
    io_registers: [u8; IO_REGISTERS_SIZE],
    timer: Timer,
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
            ppu: Ppu::default(),
            wram: [0; WRAM_SIZE],
            io_registers: [0; IO_REGISTERS_SIZE],
            timer: Timer::default(),
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
            VRAM_START..=0x9FFF => self.ppu.read_vram(address),
            EXTERNAL_RAM_START..=0xBFFF => self.cartridge.read(address),
            WRAM_START..=WRAM_END => self.wram[(address - WRAM_START) as usize],
            WRAM_ECHO_START..=WRAM_ECHO_END => self.wram[(address - WRAM_ECHO_START) as usize],
            OAM_START..=OAM_END => self.ppu.read_oam(address),
            UNUSABLE_START..=UNUSABLE_END => 0xFF,
            IO_REGISTERS_START..=IO_REGISTERS_END => {
                if address == BOOT_ROM_DISABLE_REGISTER {
                    self.boot_rom_disable_value
                } else if matches!(
                    address,
                    DIV_REGISTER | TIMA_REGISTER | TMA_REGISTER | TAC_REGISTER
                ) {
                    self.timer.read(address)
                } else if let Some(value) = self.ppu.read_register(address) {
                    value
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
        self.ppu = Ppu::default();
        self.wram = [0; WRAM_SIZE];
        self.io_registers = [0; IO_REGISTERS_SIZE];
        self.timer = Timer::default();
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
            VRAM_START..=0x9FFF => self.ppu.write_vram(address, value),
            EXTERNAL_RAM_START..=0xBFFF => self.cartridge.write(address, value),
            WRAM_START..=WRAM_END => self.wram[(address - WRAM_START) as usize] = value,
            WRAM_ECHO_START..=WRAM_ECHO_END => {
                self.wram[(address - WRAM_ECHO_START) as usize] = value
            }
            OAM_START..=OAM_END => self.ppu.write_oam(address, value),
            UNUSABLE_START..=UNUSABLE_END => {}
            IO_REGISTERS_START..=IO_REGISTERS_END => {
                if address == BOOT_ROM_DISABLE_REGISTER {
                    self.boot_rom_disable_value = value;
                    if self.boot_rom_enabled && value != 0 {
                        self.boot_rom_enabled = false;
                    }
                } else if matches!(
                    address,
                    DIV_REGISTER | TIMA_REGISTER | TMA_REGISTER | TAC_REGISTER
                ) {
                    self.timer.write(address, value);
                } else if self.ppu.write_register(address, value) {
                    if self.ppu.take_stat_irq_pending() {
                        let interrupt_flag_index =
                            (crate::interrupts::FLAG_REGISTER - IO_REGISTERS_START) as usize;
                        self.io_registers[interrupt_flag_index] |= 0x02;
                    }
                } else {
                    self.io_registers[(address - IO_REGISTERS_START) as usize] = value;
                }
            }
            HRAM_START..=HRAM_END => self.hram[(address - HRAM_START) as usize] = value,
            INTERRUPT_ENABLE_REGISTER => self.interrupt_enable = value,
        }
    }

    pub fn tick(&mut self, cycles: u32) {
        for _ in 0..cycles {
            let interrupt_flag_index =
                (crate::interrupts::FLAG_REGISTER - IO_REGISTERS_START) as usize;
            self.ppu.step(&mut self.io_registers[interrupt_flag_index]);
            self.timer
                .step(&mut self.io_registers[interrupt_flag_index]);
        }
    }

    pub const fn timer_may_generate_interrupt(&self) -> bool {
        self.timer.timer_may_generate_interrupt()
    }

    pub fn ppu_may_generate_interrupt(&self) -> bool {
        self.ppu.may_request_interrupt(self.interrupt_enable)
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

    #[test]
    fn tick_advances_ppu_and_sets_vblank_interrupt_flag() {
        let cartridge = make_cartridge(CartridgeType::RomOnly, RamSize::None);
        let mut bus = Bus::new(cartridge);
        bus.write8(crate::ppu::LCDC_REGISTER, 0x80);

        let cycles_to_vblank = 456 * 144;
        bus.tick(cycles_to_vblank);

        assert_eq!(bus.read8(crate::ppu::LY_REGISTER), 144);
        assert_ne!(bus.read8(crate::interrupts::FLAG_REGISTER) & 0x01, 0);
    }

    #[test]
    fn ppu_interrupt_generation_hint_respects_ie_state() {
        let cartridge = make_cartridge(CartridgeType::RomOnly, RamSize::None);
        let mut bus = Bus::new(cartridge);
        bus.write8(crate::ppu::LCDC_REGISTER, 0x80);

        assert!(!bus.ppu_may_generate_interrupt());

        bus.write8(0xFFFF, 0x01);
        assert!(bus.ppu_may_generate_interrupt());
    }

    #[test]
    fn enabling_active_stat_source_sets_stat_interrupt_flag_immediately() {
        let cartridge = make_cartridge(CartridgeType::RomOnly, RamSize::None);
        let mut bus = Bus::new(cartridge);
        bus.write8(crate::ppu::LCDC_REGISTER, 0x80);
        bus.tick(1);
        assert_eq!(bus.read8(crate::ppu::STAT_REGISTER) & 0x03, 0x02);
        bus.write8(crate::ppu::STAT_REGISTER, 0x20);

        assert_ne!(bus.read8(crate::interrupts::FLAG_REGISTER) & 0x02, 0);
    }
}
