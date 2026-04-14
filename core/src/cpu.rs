use crate::bus::Bus;

const FLAG_Z: u8 = 0b1000_0000;
const FLAG_N: u8 = 0b0100_0000;
const FLAG_H: u8 = 0b0010_0000;
const FLAG_C: u8 = 0b0001_0000;
const FLAGS_MASK: u8 = FLAG_Z | FLAG_N | FLAG_H | FLAG_C;

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
enum Flag {
    Zero,
    Subtract,
    HalfCarry,
    Carry,
}

impl Flag {
    const fn bit(self) -> u8 {
        match self {
            Self::Zero => FLAG_Z,
            Self::Subtract => FLAG_N,
            Self::HalfCarry => FLAG_H,
            Self::Carry => FLAG_C,
        }
    }
}

impl Registers {
    pub const fn af(&self) -> u16 {
        u16::from_be_bytes([self.a, self.f])
    }

    pub const fn bc(&self) -> u16 {
        u16::from_be_bytes([self.b, self.c])
    }

    pub const fn de(&self) -> u16 {
        u16::from_be_bytes([self.d, self.e])
    }

    pub const fn hl(&self) -> u16 {
        u16::from_be_bytes([self.h, self.l])
    }

    pub fn set_af(&mut self, value: u16) {
        let [a, f] = value.to_be_bytes();
        self.a = a;
        self.f = f & FLAGS_MASK;
    }

    pub fn set_bc(&mut self, value: u16) {
        let [b, c] = value.to_be_bytes();
        self.b = b;
        self.c = c;
    }

    pub fn set_de(&mut self, value: u16) {
        let [d, e] = value.to_be_bytes();
        self.d = d;
        self.e = e;
    }

    pub fn set_hl(&mut self, value: u16) {
        let [h, l] = value.to_be_bytes();
        self.h = h;
        self.l = l;
    }

    fn set_flag(&mut self, flag: Flag, enabled: bool) {
        if enabled {
            self.f |= flag.bit();
        } else {
            self.f &= !flag.bit();
        }
        self.f &= FLAGS_MASK;
    }
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
            0x02 | 0x12 => {
                let address = if opcode == 0x02 {
                    self.registers.bc()
                } else {
                    self.registers.de()
                };
                bus.write8(address, self.registers.a);
                8
            }
            0x03 | 0x13 | 0x23 | 0x33 => {
                match (opcode >> 4) & 0x03 {
                    0x00 => self.registers.set_bc(self.registers.bc().wrapping_add(1)),
                    0x01 => self.registers.set_de(self.registers.de().wrapping_add(1)),
                    0x02 => self.registers.set_hl(self.registers.hl().wrapping_add(1)),
                    0x03 => self.sp = self.sp.wrapping_add(1),
                    _ => unreachable!("register pair index is masked to 2 bits"),
                }
                8
            }
            0x01 | 0x11 | 0x21 | 0x31 => {
                let value = self.fetch16(bus);
                match (opcode >> 4) & 0x03 {
                    0x00 => self.registers.set_bc(value),
                    0x01 => self.registers.set_de(value),
                    0x02 => self.registers.set_hl(value),
                    0x03 => self.sp = value,
                    _ => unreachable!("register pair index is masked to 2 bits"),
                }
                12
            }
            0x0A | 0x1A => {
                let address = if opcode == 0x0A {
                    self.registers.bc()
                } else {
                    self.registers.de()
                };
                self.registers.a = bus.read8(address);
                8
            }
            0x0B | 0x1B | 0x2B | 0x3B => {
                match (opcode >> 4) & 0x03 {
                    0x00 => self.registers.set_bc(self.registers.bc().wrapping_sub(1)),
                    0x01 => self.registers.set_de(self.registers.de().wrapping_sub(1)),
                    0x02 => self.registers.set_hl(self.registers.hl().wrapping_sub(1)),
                    0x03 => self.sp = self.sp.wrapping_sub(1),
                    _ => unreachable!("register pair index is masked to 2 bits"),
                }
                8
            }
            0x09 | 0x19 | 0x29 | 0x39 => {
                let value = match (opcode >> 4) & 0x03 {
                    0x00 => self.registers.bc(),
                    0x01 => self.registers.de(),
                    0x02 => self.registers.hl(),
                    0x03 => self.sp,
                    _ => unreachable!("register pair index is masked to 2 bits"),
                };
                self.add_to_hl(value);
                8
            }
            0x22 => {
                let address = self.registers.hl();
                bus.write8(address, self.registers.a);
                self.registers.set_hl(address.wrapping_add(1));
                8
            }
            0x2A => {
                let address = self.registers.hl();
                self.registers.a = bus.read8(address);
                self.registers.set_hl(address.wrapping_add(1));
                8
            }
            0x32 => {
                let address = self.registers.hl();
                bus.write8(address, self.registers.a);
                self.registers.set_hl(address.wrapping_sub(1));
                8
            }
            0x3A => {
                let address = self.registers.hl();
                self.registers.a = bus.read8(address);
                self.registers.set_hl(address.wrapping_sub(1));
                8
            }
            0x06 | 0x0E | 0x16 | 0x1E | 0x26 | 0x2E | 0x36 | 0x3E => {
                let value = self.fetch8(bus);
                self.write_r8(opcode >> 3, value, bus);
                if (opcode >> 3) & 0x07 == 0x06 {
                    12
                } else {
                    8
                }
            }
            0x04 | 0x0C | 0x14 | 0x1C | 0x24 | 0x2C | 0x34 | 0x3C => {
                let register_index = opcode >> 3;
                let previous = self.read_r8(register_index & 0x07, bus);
                let result = previous.wrapping_add(1);
                self.write_r8(register_index & 0x07, result, bus);

                self.registers.set_flag(Flag::Zero, result == 0);
                self.registers.set_flag(Flag::Subtract, false);
                self.registers
                    .set_flag(Flag::HalfCarry, (previous & 0x0F) == 0x0F);

                if register_index & 0x07 == 0x06 {
                    12
                } else {
                    4
                }
            }
            0x05 | 0x0D | 0x15 | 0x1D | 0x25 | 0x2D | 0x35 | 0x3D => {
                let register_index = opcode >> 3;
                let previous = self.read_r8(register_index & 0x07, bus);
                let result = previous.wrapping_sub(1);
                self.write_r8(register_index & 0x07, result, bus);

                self.registers.set_flag(Flag::Zero, result == 0);
                self.registers.set_flag(Flag::Subtract, true);
                self.registers
                    .set_flag(Flag::HalfCarry, (previous & 0x0F) == 0x00);

                if register_index & 0x07 == 0x06 {
                    12
                } else {
                    4
                }
            }
            0x40..=0x7F => {
                if opcode == 0x76 {
                    self.halted = true;
                    return 4;
                }
                let source = self.read_r8(opcode & 0x07, bus);
                self.write_r8((opcode >> 3) & 0x07, source, bus);

                if ((opcode >> 3) & 0x07) == 0x06 || (opcode & 0x07) == 0x06 {
                    8
                } else {
                    4
                }
            }
            0x80..=0x87 => {
                let value = self.read_r8(opcode & 0x07, bus);
                self.add_to_a(value);
                if (opcode & 0x07) == 0x06 {
                    8
                } else {
                    4
                }
            }
            0x88..=0x8F => {
                let value = self.read_r8(opcode & 0x07, bus);
                self.adc_to_a(value);
                if (opcode & 0x07) == 0x06 {
                    8
                } else {
                    4
                }
            }
            0x90..=0x97 => {
                let value = self.read_r8(opcode & 0x07, bus);
                self.sub_from_a(value);
                if (opcode & 0x07) == 0x06 {
                    8
                } else {
                    4
                }
            }
            0x98..=0x9F => {
                let value = self.read_r8(opcode & 0x07, bus);
                self.sbc_from_a(value);
                if (opcode & 0x07) == 0x06 {
                    8
                } else {
                    4
                }
            }
            0xA0..=0xA7 => {
                let value = self.read_r8(opcode & 0x07, bus);
                self.and_with_a(value);
                if (opcode & 0x07) == 0x06 {
                    8
                } else {
                    4
                }
            }
            0xA8..=0xAF => {
                let value = self.read_r8(opcode & 0x07, bus);
                self.xor_with_a(value);
                if (opcode & 0x07) == 0x06 {
                    8
                } else {
                    4
                }
            }
            0xB0..=0xB7 => {
                let value = self.read_r8(opcode & 0x07, bus);
                self.or_with_a(value);
                if (opcode & 0x07) == 0x06 {
                    8
                } else {
                    4
                }
            }
            0xB8..=0xBF => {
                let value = self.read_r8(opcode & 0x07, bus);
                self.compare_a(value);
                if (opcode & 0x07) == 0x06 {
                    8
                } else {
                    4
                }
            }
            0xC6 => {
                let value = self.fetch8(bus);
                self.add_to_a(value);
                8
            }
            0xCE => {
                let value = self.fetch8(bus);
                self.adc_to_a(value);
                8
            }
            0xD6 => {
                let value = self.fetch8(bus);
                self.sub_from_a(value);
                8
            }
            0xDE => {
                let value = self.fetch8(bus);
                self.sbc_from_a(value);
                8
            }
            0xE6 => {
                let value = self.fetch8(bus);
                self.and_with_a(value);
                8
            }
            0xE0 => {
                let offset = self.fetch8(bus);
                bus.write8(0xFF00u16 + u16::from(offset), self.registers.a);
                12
            }
            0xE2 => {
                bus.write8(0xFF00u16 + u16::from(self.registers.c), self.registers.a);
                8
            }
            0xEA => {
                let address = self.fetch16(bus);
                bus.write8(address, self.registers.a);
                16
            }
            0xEE => {
                let value = self.fetch8(bus);
                self.xor_with_a(value);
                8
            }
            0xF6 => {
                let value = self.fetch8(bus);
                self.or_with_a(value);
                8
            }
            0xF0 => {
                let offset = self.fetch8(bus);
                self.registers.a = bus.read8(0xFF00u16 + u16::from(offset));
                12
            }
            0xF2 => {
                self.registers.a = bus.read8(0xFF00u16 + u16::from(self.registers.c));
                8
            }
            0xFA => {
                let address = self.fetch16(bus);
                self.registers.a = bus.read8(address);
                16
            }
            0xFE => {
                let value = self.fetch8(bus);
                self.compare_a(value);
                8
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

    fn read_r8(&self, register_index: u8, bus: &Bus) -> u8 {
        match register_index & 0x07 {
            0x00 => self.registers.b,
            0x01 => self.registers.c,
            0x02 => self.registers.d,
            0x03 => self.registers.e,
            0x04 => self.registers.h,
            0x05 => self.registers.l,
            0x06 => bus.read8(self.registers.hl()),
            0x07 => self.registers.a,
            _ => unreachable!("register index is masked to 3 bits"),
        }
    }

    fn write_r8(&mut self, register_index: u8, value: u8, bus: &mut Bus) {
        match register_index & 0x07 {
            0x00 => self.registers.b = value,
            0x01 => self.registers.c = value,
            0x02 => self.registers.d = value,
            0x03 => self.registers.e = value,
            0x04 => self.registers.h = value,
            0x05 => self.registers.l = value,
            0x06 => bus.write8(self.registers.hl(), value),
            0x07 => self.registers.a = value,
            _ => unreachable!("register index is masked to 3 bits"),
        }
    }

    fn add_to_a(&mut self, value: u8) {
        let previous = self.registers.a;
        let result = previous.wrapping_add(value);
        self.registers.a = result;

        self.registers.set_flag(Flag::Zero, result == 0);
        self.registers.set_flag(Flag::Subtract, false);
        self.registers
            .set_flag(Flag::HalfCarry, (previous & 0x0F) + (value & 0x0F) > 0x0F);
        self.registers
            .set_flag(Flag::Carry, u16::from(previous) + u16::from(value) > 0xFF);
    }

    fn sub_from_a(&mut self, value: u8) {
        let previous = self.registers.a;
        let result = previous.wrapping_sub(value);
        self.registers.a = result;

        self.registers.set_flag(Flag::Zero, result == 0);
        self.registers.set_flag(Flag::Subtract, true);
        self.registers
            .set_flag(Flag::HalfCarry, (previous & 0x0F) < (value & 0x0F));
        self.registers.set_flag(Flag::Carry, previous < value);
    }

    fn adc_to_a(&mut self, value: u8) {
        let carry_in = u8::from((self.registers.f & FLAG_C) != 0);
        let previous = self.registers.a;
        let result = previous.wrapping_add(value).wrapping_add(carry_in);
        self.registers.a = result;

        self.registers.set_flag(Flag::Zero, result == 0);
        self.registers.set_flag(Flag::Subtract, false);
        self.registers.set_flag(
            Flag::HalfCarry,
            (previous & 0x0F) + (value & 0x0F) + carry_in > 0x0F,
        );
        self.registers.set_flag(
            Flag::Carry,
            u16::from(previous) + u16::from(value) + u16::from(carry_in) > 0xFF,
        );
    }

    fn sbc_from_a(&mut self, value: u8) {
        let carry_in = u8::from((self.registers.f & FLAG_C) != 0);
        let previous = self.registers.a;
        let result = previous.wrapping_sub(value).wrapping_sub(carry_in);
        self.registers.a = result;

        self.registers.set_flag(Flag::Zero, result == 0);
        self.registers.set_flag(Flag::Subtract, true);
        self.registers.set_flag(
            Flag::HalfCarry,
            (previous & 0x0F) < ((value & 0x0F) + carry_in),
        );
        self.registers.set_flag(
            Flag::Carry,
            u16::from(previous) < (u16::from(value) + u16::from(carry_in)),
        );
    }

    fn and_with_a(&mut self, value: u8) {
        self.registers.a &= value;
        self.registers.set_flag(Flag::Zero, self.registers.a == 0);
        self.registers.set_flag(Flag::Subtract, false);
        self.registers.set_flag(Flag::HalfCarry, true);
        self.registers.set_flag(Flag::Carry, false);
    }

    fn xor_with_a(&mut self, value: u8) {
        self.registers.a ^= value;
        self.registers.set_flag(Flag::Zero, self.registers.a == 0);
        self.registers.set_flag(Flag::Subtract, false);
        self.registers.set_flag(Flag::HalfCarry, false);
        self.registers.set_flag(Flag::Carry, false);
    }

    fn or_with_a(&mut self, value: u8) {
        self.registers.a |= value;
        self.registers.set_flag(Flag::Zero, self.registers.a == 0);
        self.registers.set_flag(Flag::Subtract, false);
        self.registers.set_flag(Flag::HalfCarry, false);
        self.registers.set_flag(Flag::Carry, false);
    }

    fn compare_a(&mut self, value: u8) {
        let previous = self.registers.a;
        let result = previous.wrapping_sub(value);

        self.registers.set_flag(Flag::Zero, result == 0);
        self.registers.set_flag(Flag::Subtract, true);
        self.registers
            .set_flag(Flag::HalfCarry, (previous & 0x0F) < (value & 0x0F));
        self.registers.set_flag(Flag::Carry, previous < value);
    }

    fn add_to_hl(&mut self, value: u16) {
        let hl = self.registers.hl();
        let result = hl.wrapping_add(value);
        self.registers.set_hl(result);

        self.registers.set_flag(Flag::Subtract, false);
        self.registers
            .set_flag(Flag::HalfCarry, (hl & 0x0FFF) + (value & 0x0FFF) > 0x0FFF);
        self.registers
            .set_flag(Flag::Carry, u32::from(hl) + u32::from(value) > 0xFFFF);
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

    #[test]
    fn register_pair_access_round_trips() {
        let mut registers = Registers::default();

        registers.set_af(0x12F3);
        registers.set_bc(0x3456);
        registers.set_de(0x789A);
        registers.set_hl(0xBCDE);

        assert_eq!(registers.af(), 0x12F0);
        assert_eq!(registers.bc(), 0x3456);
        assert_eq!(registers.de(), 0x789A);
        assert_eq!(registers.hl(), 0xBCDE);
    }

    #[test]
    fn inc_a_flag_behavior_matches_lr35902_rules() {
        let mut cpu = Cpu::new();
        cpu.registers.a = 0xFF;
        cpu.registers.f = FLAG_C;
        let mut bus = make_bus_with_program(&[0x3C]); // INC A

        cpu.step(&mut bus);

        assert_eq!(cpu.registers.f & FLAG_Z, FLAG_Z);
        assert_eq!(cpu.registers.f & FLAG_N, 0);
        assert_eq!(cpu.registers.f & FLAG_H, FLAG_H);
        assert_eq!(cpu.registers.f & FLAG_C, FLAG_C);
    }

    #[test]
    fn ld_r_d8_and_ld_a_r_execute_expected_transfers() {
        let mut cpu = Cpu::new();
        let mut bus = make_bus_with_program(&[
            0x06, 0x12, // LD B, 12
            0x0E, 0x34, // LD C, 34
            0x78, // LD A, B
            0x4F, // LD C, A
        ]);

        cpu.step(&mut bus);
        cpu.step(&mut bus);
        cpu.step(&mut bus);
        cpu.step(&mut bus);

        assert_eq!(cpu.registers.b, 0x12);
        assert_eq!(cpu.registers.a, 0x12);
        assert_eq!(cpu.registers.c, 0x12);
    }

    #[test]
    fn alu_opcodes_update_flags_for_add_sub_and_bitwise_operations() {
        let mut cpu = Cpu::new();
        cpu.registers.a = 0x0F;
        cpu.registers.b = 0x01;
        cpu.registers.c = 0x10;
        let mut bus = make_bus_with_program(&[
            0x80, // ADD A, B => A=10, H=1
            0x91, // SUB C    => A=00, Z=1, N=1
            0xA0, // AND B    => A=00, Z=1, H=1
            0xB1, // OR C     => A=10
            0xA8, // XOR B    => A=11
            0xB9, // CP C     => compare 11-10 => C=0
        ]);

        cpu.step(&mut bus);
        assert_eq!(cpu.registers.a, 0x10);
        assert_eq!(cpu.registers.f & FLAG_H, FLAG_H);
        assert_eq!(cpu.registers.f & FLAG_N, 0);

        cpu.step(&mut bus);
        assert_eq!(cpu.registers.a, 0x00);
        assert_eq!(cpu.registers.f & FLAG_Z, FLAG_Z);
        assert_eq!(cpu.registers.f & FLAG_N, FLAG_N);

        cpu.step(&mut bus);
        assert_eq!(cpu.registers.a, 0x00);
        assert_eq!(cpu.registers.f & FLAG_H, FLAG_H);

        cpu.step(&mut bus);
        assert_eq!(cpu.registers.a, 0x10);

        cpu.step(&mut bus);
        assert_eq!(cpu.registers.a, 0x11);

        cpu.step(&mut bus);
        assert_eq!(cpu.registers.a, 0x11);
        assert_eq!(cpu.registers.f & FLAG_C, 0);
        assert_eq!(cpu.registers.f & FLAG_Z, 0);
    }

    #[test]
    fn ld_rr_d16_loads_all_16_bit_register_pairs() {
        let mut cpu = Cpu::new();
        let mut bus = make_bus_with_program(&[
            0x01, 0x34, 0x12, // LD BC, 1234
            0x11, 0x78, 0x56, // LD DE, 5678
            0x21, 0xBC, 0x9A, // LD HL, 9ABC
            0x31, 0xF0, 0xFF, // LD SP, FFF0
        ]);

        cpu.step(&mut bus);
        cpu.step(&mut bus);
        cpu.step(&mut bus);
        cpu.step(&mut bus);

        assert_eq!(cpu.registers.bc(), 0x1234);
        assert_eq!(cpu.registers.de(), 0x5678);
        assert_eq!(cpu.registers.hl(), 0x9ABC);
        assert_eq!(cpu.sp, 0xFFF0);
    }

    #[test]
    fn adc_sbc_and_immediate_alu_opcodes_execute_with_expected_flags() {
        let mut cpu = Cpu::new();
        cpu.registers.a = 0x0F;
        cpu.registers.b = 0x00;
        cpu.registers.f = FLAG_C;
        let mut bus = make_bus_with_program(&[
            0x88, // ADC A, B => 10 (carry-in consumed), H set
            0xCE, 0xEF, // ADC A, EF => FF
            0xDE, 0xF0, // SBC A, F0 => 0F, N set
            0xD6, 0x0E, // SUB 0E => 01
            0xE6, 0x01, // AND 01 => 01
            0xEE, 0x01, // XOR 01 => 00
            0xF6, 0x80, // OR 80 => 80
            0xFE, 0x80, // CP 80 => Z set, A unchanged
        ]);

        cpu.step(&mut bus);
        assert_eq!(cpu.registers.a, 0x10);
        assert_eq!(cpu.registers.f & FLAG_H, FLAG_H);
        assert_eq!(cpu.registers.f & FLAG_C, 0);

        cpu.step(&mut bus);
        assert_eq!(cpu.registers.a, 0xFF);

        cpu.step(&mut bus);
        assert_eq!(cpu.registers.a, 0x0F);
        assert_eq!(cpu.registers.f & FLAG_N, FLAG_N);

        cpu.step(&mut bus);
        assert_eq!(cpu.registers.a, 0x01);

        cpu.step(&mut bus);
        assert_eq!(cpu.registers.a, 0x01);
        assert_eq!(cpu.registers.f & FLAG_H, FLAG_H);

        cpu.step(&mut bus);
        assert_eq!(cpu.registers.a, 0x00);
        assert_eq!(cpu.registers.f & FLAG_Z, FLAG_Z);

        cpu.step(&mut bus);
        assert_eq!(cpu.registers.a, 0x80);

        cpu.step(&mut bus);
        assert_eq!(cpu.registers.a, 0x80);
        assert_eq!(cpu.registers.f & FLAG_Z, FLAG_Z);
    }

    #[test]
    fn inc_and_dec_registers_preserve_or_update_flags_like_hardware() {
        let mut cpu = Cpu::new();
        cpu.registers.b = 0x0F;
        cpu.registers.c = 0x00;
        cpu.registers.f = FLAG_C;
        let mut bus = make_bus_with_program(&[
            0x04, // INC B -> 10, H set, C preserved
            0x0D, // DEC C -> FF, H set, N set
        ]);

        cpu.step(&mut bus);
        assert_eq!(cpu.registers.b, 0x10);
        assert_eq!(cpu.registers.f & FLAG_H, FLAG_H);
        assert_eq!(cpu.registers.f & FLAG_C, FLAG_C);
        assert_eq!(cpu.registers.f & FLAG_N, 0);

        cpu.step(&mut bus);
        assert_eq!(cpu.registers.c, 0xFF);
        assert_eq!(cpu.registers.f & FLAG_N, FLAG_N);
        assert_eq!(cpu.registers.f & FLAG_H, FLAG_H);
        assert_eq!(cpu.registers.f & FLAG_C, FLAG_C);
    }

    #[test]
    fn ld_indirect_a_variants_round_trip_through_memory() {
        let mut cpu = Cpu::new();
        cpu.registers.a = 0x42;
        cpu.registers.set_bc(0xC100);
        cpu.registers.set_de(0xC101);
        cpu.registers.set_hl(0xC102);
        let mut bus = make_bus_with_program(&[
            0x02, // LD (BC),A
            0x12, // LD (DE),A
            0x22, // LD (HL+),A
            0x3E, 0x00, // LD A,00
            0x0A, // LD A,(BC)
            0x1A, // LD A,(DE)
            0x2A, // LD A,(HL+) ; reads C103 (default 00)
        ]);

        for _ in 0..7 {
            cpu.step(&mut bus);
        }

        assert_eq!(bus.read8(0xC100), 0x42);
        assert_eq!(bus.read8(0xC101), 0x42);
        assert_eq!(bus.read8(0xC102), 0x42);
        assert_eq!(cpu.registers.a, 0x00);
        assert_eq!(cpu.registers.hl(), 0xC104);
    }

    #[test]
    fn ldh_and_absolute_a_transfers_work() {
        let mut cpu = Cpu::new();
        cpu.registers.a = 0x9C;
        cpu.registers.c = 0x12;
        let mut bus = make_bus_with_program(&[
            0xE0, 0x80, // LDH (80),A
            0xE2, // LD (C),A
            0xEA, 0x34, 0xC2, // LD (C234),A
            0x3E, 0x00, // LD A,00
            0xF0, 0x80, // LDH A,(80)
            0xF2, // LD A,(C)
            0xFA, 0x34, 0xC2, // LD A,(C234)
        ]);

        for _ in 0..7 {
            cpu.step(&mut bus);
        }

        assert_eq!(bus.read8(0xFF80), 0x9C);
        assert_eq!(bus.read8(0xFF12), 0x9C);
        assert_eq!(bus.read8(0xC234), 0x9C);
        assert_eq!(cpu.registers.a, 0x9C);
    }

    #[test]
    fn sixteen_bit_inc_dec_and_add_hl_follow_expected_rules() {
        let mut cpu = Cpu::new();
        cpu.registers.set_bc(0x0FFF);
        cpu.registers.set_de(0x0001);
        cpu.registers.set_hl(0x8FFF);
        cpu.sp = 0xFFFF;
        cpu.registers.f = FLAG_Z;
        let mut bus = make_bus_with_program(&[
            0x03, // INC BC
            0x13, // INC DE
            0x33, // INC SP
            0x0B, // DEC BC
            0x1B, // DEC DE
            0x3B, // DEC SP
            0x09, // ADD HL,BC
            0x19, // ADD HL,DE
            0x39, // ADD HL,SP
        ]);

        for _ in 0..9 {
            cpu.step(&mut bus);
        }

        assert_eq!(cpu.registers.bc(), 0x0FFF);
        assert_eq!(cpu.registers.de(), 0x0001);
        assert_eq!(cpu.sp, 0xFFFF);
        assert_eq!(cpu.registers.hl(), 0x9FFE);
        assert_eq!(cpu.registers.f & FLAG_Z, FLAG_Z);
        assert_eq!(cpu.registers.f & FLAG_N, 0);
        assert_eq!(cpu.registers.f & FLAG_H, FLAG_H);
        assert_eq!(cpu.registers.f & FLAG_C, FLAG_C);
    }
}
