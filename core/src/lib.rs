pub mod apu;
pub mod bus;
pub mod cartridge;
pub mod cpu;
pub mod frontend;
pub mod input;
pub mod interrupts;
pub mod ppu;
pub mod serial;
pub mod timer;

use bus::Bus;
use cartridge::{
    compute_header_checksum, Cartridge, CartridgeType, DestinationCode, RamSize, RomSize,
};
use cpu::Cpu;
use std::hash::{Hash, Hasher};

/// Top-level emulator state container for subsystem wiring.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Emulator {
    cpu: Cpu,
    bus: Bus,
    total_cycles: u64,
}

impl Hash for Emulator {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.cpu.hash(state);
        self.total_cycles.hash(state);
    }
}

impl Default for Emulator {
    fn default() -> Self {
        Self::new()
    }
}

impl Emulator {
    /// Creates a new emulator with a minimal ROM-only cartridge.
    pub fn new() -> Self {
        Self::from_cartridge(default_rom_only_cartridge())
    }

    /// Creates a new emulator from a cartridge image.
    pub fn from_cartridge(cartridge: Cartridge) -> Self {
        Self {
            cpu: Cpu::new(),
            bus: Bus::new(cartridge),
            total_cycles: 0,
        }
    }

    /// Resets emulator state to defaults.
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// Advances execution by at least `cycles` machine cycles.
    pub fn step_cycles(&mut self, cycles: u32) {
        let mut executed = 0u32;
        while executed < cycles {
            executed = executed.wrapping_add(self.cpu.step(&mut self.bus));
        }

        self.total_cycles = self.total_cycles.wrapping_add(executed as u64);
    }

    pub const fn cpu(&self) -> &Cpu {
        &self.cpu
    }

    pub const fn bus(&self) -> &Bus {
        &self.bus
    }

    /// Returns total cycles executed by this emulator instance.
    pub const fn total_cycles(&self) -> u64 {
        self.total_cycles
    }
}

fn default_rom_only_cartridge() -> Cartridge {
    let mut rom = vec![0u8; 2 * 16 * 1024];
    rom[0x0000] = 0x76; // HALT
    rom[0x0134..0x0138].copy_from_slice(b"EMUT");
    rom[0x0147] = CartridgeType::RomOnly.code();
    rom[0x0148] = RomSize::Banks2.code();
    rom[0x0149] = RamSize::None.code();
    rom[0x014A] = DestinationCode::Japanese.code();
    rom[0x014D] =
        compute_header_checksum(&rom).expect("default rom header checksum should compute");

    Cartridge::from_rom(rom).expect("default rom should parse")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rom_boot_smoke_executes_instruction_stream() {
        let mut rom = vec![0u8; 2 * 16 * 1024];
        rom[0x0000] = 0x31; // LD SP, d16
        rom[0x0001] = 0x00;
        rom[0x0002] = 0xC0;
        rom[0x0003] = 0x3E; // LD A, d8
        rom[0x0004] = 0x42;
        rom[0x0005] = 0xEA; // LD (a16), A
        rom[0x0006] = 0x00;
        rom[0x0007] = 0xC0;
        rom[0x0008] = 0x3C; // INC A
        rom[0x0009] = 0xEA; // LD (a16), A
        rom[0x000A] = 0x01;
        rom[0x000B] = 0xC0;
        rom[0x000C] = 0x76; // HALT

        rom[0x0134..0x0138].copy_from_slice(b"SMOK");
        rom[0x0147] = CartridgeType::RomOnly.code();
        rom[0x0148] = RomSize::Banks2.code();
        rom[0x0149] = RamSize::None.code();
        rom[0x014A] = DestinationCode::Japanese.code();
        rom[0x014D] =
            compute_header_checksum(&rom).expect("test rom header checksum should compute");

        let cartridge = Cartridge::from_rom(rom).expect("test rom should parse");
        let mut emulator = Emulator::from_cartridge(cartridge);

        emulator.step_cycles(64);

        assert!(emulator.cpu().halted());
        assert_eq!(emulator.cpu().pc(), 0x000D);
        assert_eq!(emulator.cpu().sp(), 0xC000);
        assert_eq!(emulator.cpu().registers().a, 0x43);
        assert_eq!(emulator.bus().read8(0xC000), 0x42);
        assert_eq!(emulator.bus().read8(0xC001), 0x43);
    }
}
