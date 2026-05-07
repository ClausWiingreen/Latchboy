//! Memory-map routing primitives for the DMG address bus.
//!
//! The CPU still talks to [`crate::bus::Bus`] through `read8`/`write8`, but the
//! address decoding lives here so device ownership and register implementations
//! stay behind route entries instead of one monolithic match in the bus API.

use crate::apu::Apu;
use crate::input::Joypad;
use crate::ppu::Ppu;
use crate::serial::SerialPort;
use crate::timer::{Timer, DIV_REGISTER, TAC_REGISTER, TIMA_REGISTER, TMA_REGISTER};

pub const ROM_START: u16 = 0x0000;
pub const ROM_END: u16 = 0x7FFF;
pub const VRAM_START: u16 = 0x8000;
pub const VRAM_END: u16 = 0x9FFF;
pub const EXTERNAL_RAM_START: u16 = 0xA000;
pub const EXTERNAL_RAM_END: u16 = 0xBFFF;
pub const WRAM_START: u16 = 0xC000;
pub const WRAM_END: u16 = 0xDFFF;
pub const WRAM_ECHO_START: u16 = 0xE000;
pub const WRAM_ECHO_END: u16 = 0xFDFF;
pub const OAM_START: u16 = 0xFE00;
pub const OAM_END: u16 = 0xFE9F;
pub const UNUSABLE_START: u16 = 0xFEA0;
pub const UNUSABLE_END: u16 = 0xFEFF;
pub const IO_REGISTERS_START: u16 = 0xFF00;
pub const IO_REGISTERS_END: u16 = 0xFF7F;
pub const JOYP_REGISTER: u16 = 0xFF00;
pub const BOOT_ROM_DISABLE_REGISTER: u16 = 0xFF50;
pub const CGB_SPEED_SWITCH_REGISTER: u16 = 0xFF4D;
pub const HRAM_START: u16 = 0xFF80;
pub const HRAM_END: u16 = 0xFFFE;
pub const INTERRUPT_ENABLE_REGISTER: u16 = 0xFFFF;
pub const BOOT_ROM_SIZE: usize = 0x100;
pub const WRAM_SIZE: usize = 0x2000;
pub const IO_REGISTERS_SIZE: usize = 0x80;
pub const HRAM_SIZE: usize = 0x7F;

pub trait MemoryMappedDevice {
    fn read8(&self, address: u16) -> u8;
    fn write8(&mut self, address: u16, value: u8);

    fn tick(&mut self, _cycles: u32, _interrupts: &mut u8) {}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RouteEntry {
    pub start: u16,
    pub end: u16,
    pub target: BusRoute,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BusRoute {
    BootRomOrCartridge,
    Cartridge,
    PpuVram,
    WorkRam,
    WorkRamEcho,
    PpuOam,
    Unusable,
    Io,
    HighRam,
    InterruptEnable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IoRoute {
    BootRomDisable,
    CgbSpeedSwitch,
    Joypad,
    Apu,
    Serial,
    Timer,
    Ppu,
    GenericIo,
}

const BUS_ROUTE_ENTRIES: &[RouteEntry] = &[
    RouteEntry {
        start: ROM_START,
        end: ROM_END,
        target: BusRoute::BootRomOrCartridge,
    },
    RouteEntry {
        start: VRAM_START,
        end: VRAM_END,
        target: BusRoute::PpuVram,
    },
    RouteEntry {
        start: EXTERNAL_RAM_START,
        end: EXTERNAL_RAM_END,
        target: BusRoute::Cartridge,
    },
    RouteEntry {
        start: WRAM_START,
        end: WRAM_END,
        target: BusRoute::WorkRam,
    },
    RouteEntry {
        start: WRAM_ECHO_START,
        end: WRAM_ECHO_END,
        target: BusRoute::WorkRamEcho,
    },
    RouteEntry {
        start: OAM_START,
        end: OAM_END,
        target: BusRoute::PpuOam,
    },
    RouteEntry {
        start: UNUSABLE_START,
        end: UNUSABLE_END,
        target: BusRoute::Unusable,
    },
    RouteEntry {
        start: IO_REGISTERS_START,
        end: IO_REGISTERS_END,
        target: BusRoute::Io,
    },
    RouteEntry {
        start: HRAM_START,
        end: HRAM_END,
        target: BusRoute::HighRam,
    },
    RouteEntry {
        start: INTERRUPT_ENABLE_REGISTER,
        end: INTERRUPT_ENABLE_REGISTER,
        target: BusRoute::InterruptEnable,
    },
];

pub fn route_address(address: u16) -> BusRoute {
    BUS_ROUTE_ENTRIES
        .iter()
        .find(|entry| (entry.start..=entry.end).contains(&address))
        .map(|entry| entry.target)
        .expect("all 16-bit addresses are covered by bus route entries")
}

pub fn route_io_address(address: u16) -> IoRoute {
    if address == BOOT_ROM_DISABLE_REGISTER {
        IoRoute::BootRomDisable
    } else if address == CGB_SPEED_SWITCH_REGISTER {
        IoRoute::CgbSpeedSwitch
    } else if address == JOYP_REGISTER {
        IoRoute::Joypad
    } else if matches!(address, 0xFF10..=0xFF26 | 0xFF30..=0xFF3F) {
        IoRoute::Apu
    } else if matches!(
        address,
        crate::serial::SB_REGISTER | crate::serial::SC_REGISTER
    ) {
        IoRoute::Serial
    } else if matches!(
        address,
        DIV_REGISTER | TIMA_REGISTER | TMA_REGISTER | TAC_REGISTER
    ) {
        IoRoute::Timer
    } else if matches!(
        address,
        crate::ppu::LCDC_REGISTER
            | crate::ppu::STAT_REGISTER
            | crate::ppu::SCY_REGISTER
            | crate::ppu::SCX_REGISTER
            | crate::ppu::LY_REGISTER
            | crate::ppu::LYC_REGISTER
            | crate::ppu::DMA_REGISTER
            | crate::ppu::BGP_REGISTER
            | crate::ppu::OBP0_REGISTER
            | crate::ppu::OBP1_REGISTER
            | crate::ppu::WY_REGISTER
            | crate::ppu::WX_REGISTER
            | crate::ppu::VBK_REGISTER
            | crate::ppu::BCPS_REGISTER
            | crate::ppu::BCPD_REGISTER
            | crate::ppu::OCPS_REGISTER
            | crate::ppu::OCPD_REGISTER
    ) {
        IoRoute::Ppu
    } else {
        IoRoute::GenericIo
    }
}

impl MemoryMappedDevice for Ppu {
    fn read8(&self, address: u16) -> u8 {
        match route_address(address) {
            BusRoute::PpuVram => self.read_vram(address),
            BusRoute::PpuOam => self.read_oam(address),
            BusRoute::Io if route_io_address(address) == IoRoute::Ppu => {
                self.read_register(address).unwrap_or(0xFF)
            }
            _ => 0xFF,
        }
    }

    fn write8(&mut self, address: u16, value: u8) {
        match route_address(address) {
            BusRoute::PpuVram => self.write_vram(address, value),
            BusRoute::PpuOam => self.write_oam(address, value),
            BusRoute::Io if route_io_address(address) == IoRoute::Ppu => {
                let _ = self.write_register(address, value);
            }
            _ => {}
        }
    }

    fn tick(&mut self, cycles: u32, interrupts: &mut u8) {
        for _ in 0..cycles {
            self.step(interrupts);
        }
    }
}

impl MemoryMappedDevice for Apu {
    fn read8(&self, address: u16) -> u8 {
        self.read_register(address).unwrap_or(0xFF)
    }

    fn write8(&mut self, address: u16, value: u8) {
        let _ = self.write_register(address, value);
    }

    fn tick(&mut self, cycles: u32, _interrupts: &mut u8) {
        let _ = self.tick(cycles);
    }
}

impl MemoryMappedDevice for Timer {
    fn read8(&self, address: u16) -> u8 {
        self.read(address)
    }

    fn write8(&mut self, address: u16, value: u8) {
        self.write(address, value);
    }

    fn tick(&mut self, cycles: u32, interrupts: &mut u8) {
        for _ in 0..cycles {
            self.step(interrupts);
        }
    }
}

impl MemoryMappedDevice for SerialPort {
    fn read8(&self, address: u16) -> u8 {
        self.read(address).unwrap_or(0xFF)
    }

    fn write8(&mut self, address: u16, value: u8) {
        let _ = self.write(address, value);
    }
}

impl MemoryMappedDevice for Joypad {
    fn read8(&self, _address: u16) -> u8 {
        self.read_p1()
    }

    fn write8(&mut self, address: u16, value: u8) {
        debug_assert_eq!(address, JOYP_REGISTER);
        let _ = self.write_p1(value);
    }
}
