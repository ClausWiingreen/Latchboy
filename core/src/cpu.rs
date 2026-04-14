use crate::bus::Bus;

const FLAG_Z: u8 = 0b1000_0000;
const FLAG_N: u8 = 0b0100_0000;
const FLAG_H: u8 = 0b0010_0000;
const FLAG_C: u8 = 0b0001_0000;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Registers {
    pub a: u8,
    pub f: u8,
    pub b: u8,
    pub c: u8,
    pub d: u8,
    pub e: u8,
    pub h: u8,
    pub l: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Cpu {
    registers: Registers,
    pc: u16,
    sp: u16,
    halted: bool,
}

impl Default for Cpu {
    fn default() -> Self {
        Self::new()
    }
}

impl Cpu {
    pub const fn new() -> Self {
        Self {
            registers: Registers {
                a: 0,
                f: 0,
                b: 0,
                c: 0,
                d: 0,
                e: 0,
                h: 0,
                l: 0,
            },
            pc: 0x0000,
            sp: 0xFFFE,
            halted: false,
        }
    }

    pub const fn registers(&self) -> &Registers {
        &self.registers
    }

    pub const fn pc(&self) -> u16 {
        self.pc
    }

    pub const fn sp(&self) -> u16 {
        self.sp
    }

    pub const fn halted(&self) -> bool {
        self.halted
    }

    pub fn step(&mut self, bus: &mut Bus) -> u32 {
        if self.halted {
            return 4;
        }

        let opcode = self.fetch8(bus);
        match opcode {
            0x00 => 4, // NOP
            0x06 => {
                self.registers.b = self.fetch8(bus);
                8
            }
            0x0E => {
                self.registers.c = self.fetch8(bus);
                8
            }
            0x31 => {
                self.sp = self.fetch16(bus);
                12
            }
            0x3C => {
                let previous = self.registers.a;
                let result = previous.wrapping_add(1);
                self.registers.a = result;

                let mut flags = self.registers.f & FLAG_C;
                flags &= !FLAG_N;
                if result == 0 {
                    flags |= FLAG_Z;
                }
                if (previous & 0x0F) == 0x0F {
                    flags |= FLAG_H;
                }
                self.registers.f = flags;

                4
            }
            0x3E => {
                self.registers.a = self.fetch8(bus);
                8
            }
            0x76 => {
                self.halted = true;
                4
            }
            0xEA => {
                let address = self.fetch16(bus);
                bus.write8(address, self.registers.a);
                16
            }
            0xC3 => {
                let address = self.fetch16(bus);
                self.pc = address;
                16
            }
            _ => panic!("unimplemented opcode: 0x{opcode:02X}"),
        }
    }

    fn fetch8(&mut self, bus: &Bus) -> u8 {
        let value = bus.read8(self.pc);
        self.pc = self.pc.wrapping_add(1);
        value
    }

    fn fetch16(&mut self, bus: &Bus) -> u16 {
        let lo = self.fetch8(bus) as u16;
        let hi = self.fetch8(bus) as u16;
        (hi << 8) | lo
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::{
        compute_header_checksum, Cartridge, CartridgeType, DestinationCode, RamSize, RomSize,
    };

    fn make_bus_with_program(program: &[u8]) -> Bus {
        let mut rom = vec![0u8; 2 * 16 * 1024];
        rom[..program.len()].copy_from_slice(program);
        rom[0x0134..0x0138].copy_from_slice(b"CPUT");
        rom[0x0147] = CartridgeType::RomOnly.code();
        rom[0x0148] = RomSize::Banks2.code();
        rom[0x0149] = RamSize::None.code();
        rom[0x014A] = DestinationCode::Japanese.code();
        rom[0x014D] = compute_header_checksum(&rom).expect("header checksum should compute");

        let cartridge = Cartridge::from_rom(rom).expect("test rom should parse");
        Bus::new(cartridge)
    }

    #[test]
    fn inc_a_sets_z_and_h_and_clears_n() {
        let mut cpu = Cpu::new();
        cpu.registers.a = 0xFF;
        cpu.registers.f = FLAG_C | FLAG_N;
        let mut bus = make_bus_with_program(&[0x3C]); // INC A

        cpu.step(&mut bus);

        assert_eq!(cpu.registers.a, 0x00);
        assert_eq!(cpu.registers.f & FLAG_Z, FLAG_Z);
        assert_eq!(cpu.registers.f & FLAG_H, FLAG_H);
        assert_eq!(cpu.registers.f & FLAG_N, 0);
        assert_eq!(cpu.registers.f & FLAG_C, FLAG_C);
    }

    #[test]
    fn inc_a_clears_z_when_result_non_zero() {
        let mut cpu = Cpu::new();
        cpu.registers.a = 0x0E;
        cpu.registers.f = FLAG_Z | FLAG_C;
        let mut bus = make_bus_with_program(&[0x3C]); // INC A

        cpu.step(&mut bus);

        assert_eq!(cpu.registers.a, 0x0F);
        assert_eq!(cpu.registers.f & FLAG_Z, 0);
        assert_eq!(cpu.registers.f & FLAG_H, 0);
        assert_eq!(cpu.registers.f & FLAG_C, FLAG_C);
    }
}
