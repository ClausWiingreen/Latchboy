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
    ime: bool,
    ime_enable_pending: bool,
    last_unimplemented_opcode: Option<u8>,
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
            ime: false,
            ime_enable_pending: false,
            last_unimplemented_opcode: None,
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

    pub const fn ime(&self) -> bool {
        self.ime
    }

    pub const fn last_unimplemented_opcode(&self) -> Option<u8> {
        self.last_unimplemented_opcode
    }

    pub fn step(&mut self, bus: &mut Bus) -> u32 {
        if self.ime_enable_pending {
            self.ime = true;
            self.ime_enable_pending = false;
        }

        if self.halted {
            return 4;
        }

        let opcode = self.fetch8(bus);
        match opcode {
            0x00 => 4, // NOP
            0x07 => {
                self.rlca();
                4
            }
            0x08 => {
                let address = self.fetch16(bus);
                let [lo, hi] = self.sp.to_le_bytes();
                bus.write8(address, lo);
                bus.write8(address.wrapping_add(1), hi);
                20
            }
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
            0x0F => {
                self.rrca();
                4
            }
            0x10 => {
                let _ = self.fetch8(bus);
                self.halted = true;
                4
            }
            0x18 => {
                let offset = self.fetch8(bus) as i8;
                self.pc = self.pc.wrapping_add_signed(i16::from(offset));
                12
            }
            0x20 | 0x28 | 0x30 | 0x38 => {
                let offset = self.fetch8(bus) as i8;
                if self.condition_met((opcode >> 3) & 0x03) {
                    self.pc = self.pc.wrapping_add_signed(i16::from(offset));
                    12
                } else {
                    8
                }
            }
            0x17 => {
                self.rla();
                4
            }
            0x1F => {
                self.rra();
                4
            }
            0x27 => {
                self.daa();
                4
            }
            0x2F => {
                self.registers.a = !self.registers.a;
                self.registers.set_flag(Flag::Subtract, true);
                self.registers.set_flag(Flag::HalfCarry, true);
                4
            }
            0x37 => {
                self.registers.set_flag(Flag::Subtract, false);
                self.registers.set_flag(Flag::HalfCarry, false);
                self.registers.set_flag(Flag::Carry, true);
                4
            }
            0x3F => {
                let carry = (self.registers.f & FLAG_C) == 0;
                self.registers.set_flag(Flag::Subtract, false);
                self.registers.set_flag(Flag::HalfCarry, false);
                self.registers.set_flag(Flag::Carry, carry);
                4
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
            0xC0 | 0xC8 | 0xD0 | 0xD8 => {
                if self.condition_met((opcode >> 3) & 0x03) {
                    self.pc = self.pop_stack16(bus);
                    20
                } else {
                    8
                }
            }
            0xC1 | 0xD1 | 0xE1 | 0xF1 => {
                let value = self.pop_stack16(bus);
                match (opcode >> 4) & 0x03 {
                    0x00 => self.registers.set_bc(value),
                    0x01 => self.registers.set_de(value),
                    0x02 => self.registers.set_hl(value),
                    0x03 => self.registers.set_af(value),
                    _ => unreachable!("register pair index is masked to 2 bits"),
                }
                12
            }
            0xC2 | 0xCA | 0xD2 | 0xDA => {
                let address = self.fetch16(bus);
                if self.condition_met((opcode >> 3) & 0x03) {
                    self.pc = address;
                    16
                } else {
                    12
                }
            }
            0xCE => {
                let value = self.fetch8(bus);
                self.adc_to_a(value);
                8
            }
            0xC3 => {
                let address = self.fetch16(bus);
                self.pc = address;
                16
            }
            0xC4 | 0xCC | 0xD4 | 0xDC => {
                let address = self.fetch16(bus);
                if self.condition_met((opcode >> 3) & 0x03) {
                    self.push_stack16(bus, self.pc);
                    self.pc = address;
                    24
                } else {
                    12
                }
            }
            0xC5 | 0xD5 | 0xE5 | 0xF5 => {
                let value = match (opcode >> 4) & 0x03 {
                    0x00 => self.registers.bc(),
                    0x01 => self.registers.de(),
                    0x02 => self.registers.hl(),
                    0x03 => self.registers.af(),
                    _ => unreachable!("register pair index is masked to 2 bits"),
                };
                self.push_stack16(bus, value);
                16
            }
            0xC7 | 0xCF | 0xD7 | 0xDF | 0xE7 | 0xEF | 0xF7 | 0xFF => {
                let vector = u16::from(opcode & 0x38);
                self.push_stack16(bus, self.pc);
                self.pc = vector;
                16
            }
            0xCB => {
                let cb_opcode = self.fetch8(bus);
                self.execute_cb(cb_opcode, bus)
            }
            0xC9 => {
                self.pc = self.pop_stack16(bus);
                16
            }
            0xCD => {
                let address = self.fetch16(bus);
                self.push_stack16(bus, self.pc);
                self.pc = address;
                24
            }
            0xD6 => {
                let value = self.fetch8(bus);
                self.sub_from_a(value);
                8
            }
            0xD9 => {
                self.pc = self.pop_stack16(bus);
                self.ime = true;
                self.ime_enable_pending = false;
                16
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
            0xE8 => {
                let offset = self.fetch8(bus) as i8;
                self.sp = self.add_signed_to_sp(offset);
                16
            }
            0xE9 => {
                self.pc = self.registers.hl();
                4
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
            0xF8 => {
                let offset = self.fetch8(bus) as i8;
                let result = self.add_signed_to_sp(offset);
                self.registers.set_hl(result);
                12
            }
            0xF9 => {
                self.sp = self.registers.hl();
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
            0xF3 => {
                self.ime = false;
                self.ime_enable_pending = false;
                4
            }
            0xFA => {
                let address = self.fetch16(bus);
                self.registers.a = bus.read8(address);
                16
            }
            0xFB => {
                self.ime_enable_pending = true;
                4
            }
            0xFE => {
                let value = self.fetch8(bus);
                self.compare_a(value);
                8
            }
            _ => self.handle_unimplemented_opcode(opcode),
        }
    }

    fn handle_unimplemented_opcode(&mut self, opcode: u8) -> u32 {
        self.halted = true;
        self.last_unimplemented_opcode = Some(opcode);
        4
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

    fn push_stack16(&mut self, bus: &mut Bus, value: u16) {
        let [lo, hi] = value.to_le_bytes();
        self.sp = self.sp.wrapping_sub(1);
        bus.write8(self.sp, hi);
        self.sp = self.sp.wrapping_sub(1);
        bus.write8(self.sp, lo);
    }

    fn pop_stack16(&mut self, bus: &Bus) -> u16 {
        let lo = bus.read8(self.sp);
        self.sp = self.sp.wrapping_add(1);
        let hi = bus.read8(self.sp);
        self.sp = self.sp.wrapping_add(1);
        u16::from_le_bytes([lo, hi])
    }

    fn condition_met(&self, condition_index: u8) -> bool {
        match condition_index & 0x03 {
            0x00 => (self.registers.f & FLAG_Z) == 0,
            0x01 => (self.registers.f & FLAG_Z) != 0,
            0x02 => (self.registers.f & FLAG_C) == 0,
            0x03 => (self.registers.f & FLAG_C) != 0,
            _ => unreachable!("condition index is masked to 2 bits"),
        }
    }

    fn add_signed_to_sp(&mut self, offset: i8) -> u16 {
        let sp = self.sp;
        let signed = i16::from(offset);
        let result = sp.wrapping_add_signed(signed);
        let offset_u16 = u16::from(offset as u8);
        self.registers.set_flag(Flag::Zero, false);
        self.registers.set_flag(Flag::Subtract, false);
        self.registers.set_flag(
            Flag::HalfCarry,
            (sp & 0x000F) + (offset_u16 & 0x000F) > 0x000F,
        );
        self.registers
            .set_flag(Flag::Carry, (sp & 0x00FF) + (offset_u16 & 0x00FF) > 0x00FF);
        result
    }

    fn execute_cb(&mut self, opcode: u8, bus: &mut Bus) -> u32 {
        let register_index = opcode & 0x07;
        let bit_index = (opcode >> 3) & 0x07;
        match opcode >> 6 {
            0x00 => {
                let value = self.read_r8(register_index, bus);
                let (result, carry) = match bit_index {
                    0x00 => (value.rotate_left(1), (value & 0x80) != 0), // RLC
                    0x01 => (value.rotate_right(1), (value & 0x01) != 0), // RRC
                    0x02 => {
                        let carry_in = u8::from((self.registers.f & FLAG_C) != 0);
                        ((value << 1) | carry_in, (value & 0x80) != 0) // RL
                    }
                    0x03 => {
                        let carry_in = if (self.registers.f & FLAG_C) != 0 {
                            0x80
                        } else {
                            0x00
                        };
                        ((value >> 1) | carry_in, (value & 0x01) != 0) // RR
                    }
                    0x04 => (value << 1, (value & 0x80) != 0), // SLA
                    0x05 => (((value >> 1) | (value & 0x80)), (value & 0x01) != 0), // SRA
                    0x06 => (value.rotate_left(4), false),     // SWAP
                    0x07 => (value >> 1, (value & 0x01) != 0), // SRL
                    _ => unreachable!("bit index is masked to 3 bits"),
                };

                self.write_r8(register_index, result, bus);
                self.registers.set_flag(Flag::Zero, result == 0);
                self.registers.set_flag(Flag::Subtract, false);
                self.registers.set_flag(Flag::HalfCarry, false);
                self.registers.set_flag(Flag::Carry, carry);

                if register_index == 0x06 {
                    16
                } else {
                    8
                }
            }
            0x01 => {
                let value = self.read_r8(register_index, bus);
                self.registers
                    .set_flag(Flag::Zero, (value & (1 << bit_index)) == 0);
                self.registers.set_flag(Flag::Subtract, false);
                self.registers.set_flag(Flag::HalfCarry, true);
                if register_index == 0x06 {
                    12
                } else {
                    8
                }
            }
            0x02 => {
                let value = self.read_r8(register_index, bus) & !(1 << bit_index);
                self.write_r8(register_index, value, bus);
                if register_index == 0x06 {
                    16
                } else {
                    8
                }
            }
            0x03 => {
                let value = self.read_r8(register_index, bus) | (1 << bit_index);
                self.write_r8(register_index, value, bus);
                if register_index == 0x06 {
                    16
                } else {
                    8
                }
            }
            _ => unreachable!("cb opcode group is masked to 2 bits"),
        }
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

    fn rlca(&mut self) {
        let carry = (self.registers.a & 0x80) != 0;
        self.registers.a = self.registers.a.rotate_left(1);
        self.registers.set_flag(Flag::Zero, false);
        self.registers.set_flag(Flag::Subtract, false);
        self.registers.set_flag(Flag::HalfCarry, false);
        self.registers.set_flag(Flag::Carry, carry);
    }

    fn rrca(&mut self) {
        let carry = (self.registers.a & 0x01) != 0;
        self.registers.a = self.registers.a.rotate_right(1);
        self.registers.set_flag(Flag::Zero, false);
        self.registers.set_flag(Flag::Subtract, false);
        self.registers.set_flag(Flag::HalfCarry, false);
        self.registers.set_flag(Flag::Carry, carry);
    }

    fn rla(&mut self) {
        let carry_in = u8::from((self.registers.f & FLAG_C) != 0);
        let carry_out = (self.registers.a & 0x80) != 0;
        self.registers.a = (self.registers.a << 1) | carry_in;
        self.registers.set_flag(Flag::Zero, false);
        self.registers.set_flag(Flag::Subtract, false);
        self.registers.set_flag(Flag::HalfCarry, false);
        self.registers.set_flag(Flag::Carry, carry_out);
    }

    fn rra(&mut self) {
        let carry_in = if (self.registers.f & FLAG_C) != 0 {
            0x80
        } else {
            0x00
        };
        let carry_out = (self.registers.a & 0x01) != 0;
        self.registers.a = (self.registers.a >> 1) | carry_in;
        self.registers.set_flag(Flag::Zero, false);
        self.registers.set_flag(Flag::Subtract, false);
        self.registers.set_flag(Flag::HalfCarry, false);
        self.registers.set_flag(Flag::Carry, carry_out);
    }

    fn daa(&mut self) {
        let mut adjust = 0u8;
        let mut set_carry = false;

        if (self.registers.f & FLAG_N) == 0 {
            if (self.registers.f & FLAG_H) != 0 || (self.registers.a & 0x0F) > 0x09 {
                adjust |= 0x06;
            }
            if (self.registers.f & FLAG_C) != 0 || self.registers.a > 0x99 {
                adjust |= 0x60;
                set_carry = true;
            }
            self.registers.a = self.registers.a.wrapping_add(adjust);
        } else {
            if (self.registers.f & FLAG_H) != 0 {
                adjust |= 0x06;
            }
            if (self.registers.f & FLAG_C) != 0 {
                adjust |= 0x60;
            }
            self.registers.a = self.registers.a.wrapping_sub(adjust);
            set_carry = (self.registers.f & FLAG_C) != 0;
        }

        self.registers.set_flag(Flag::Zero, self.registers.a == 0);
        self.registers.set_flag(Flag::HalfCarry, false);
        self.registers.set_flag(Flag::Carry, set_carry);
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

    #[test]
    fn accumulator_rotate_opcodes_use_expected_carry_paths() {
        let mut cpu = Cpu::new();
        cpu.registers.a = 0x85;
        cpu.registers.f = FLAG_Z;
        let mut bus = make_bus_with_program(&[
            0x07, // RLCA: 85 -> 0B, C=1
            0x0F, // RRCA: 0B -> 85, C=1
            0x17, // RLA: carry-in 1, 85 -> 0B, C=1
            0x1F, // RRA: carry-in 1, 0B -> 85, C=1
        ]);

        cpu.step(&mut bus);
        assert_eq!(cpu.registers.a, 0x0B);
        assert_eq!(cpu.registers.f & FLAG_C, FLAG_C);
        assert_eq!(cpu.registers.f & FLAG_Z, 0);

        cpu.step(&mut bus);
        assert_eq!(cpu.registers.a, 0x85);
        assert_eq!(cpu.registers.f & FLAG_C, FLAG_C);

        cpu.step(&mut bus);
        assert_eq!(cpu.registers.a, 0x0B);
        assert_eq!(cpu.registers.f & FLAG_C, FLAG_C);

        cpu.step(&mut bus);
        assert_eq!(cpu.registers.a, 0x85);
        assert_eq!(cpu.registers.f & FLAG_C, FLAG_C);
    }

    #[test]
    fn daa_cpl_scf_and_ccf_update_accumulator_and_flags() {
        let mut cpu = Cpu::new();
        let mut bus = make_bus_with_program(&[
            0x3E, 0x9A, // LD A,9A
            0x27, // DAA -> 00 with carry
            0x2F, // CPL -> FF, set N/H
            0x37, // SCF -> C=1, N/H cleared
            0x3F, // CCF -> C=0, N/H cleared
        ]);

        cpu.step(&mut bus);
        cpu.step(&mut bus);
        assert_eq!(cpu.registers.a, 0x00);
        assert_eq!(cpu.registers.f & FLAG_Z, FLAG_Z);
        assert_eq!(cpu.registers.f & FLAG_C, FLAG_C);
        assert_eq!(cpu.registers.f & FLAG_H, 0);

        cpu.step(&mut bus);
        assert_eq!(cpu.registers.a, 0xFF);
        assert_eq!(cpu.registers.f & FLAG_N, FLAG_N);
        assert_eq!(cpu.registers.f & FLAG_H, FLAG_H);

        cpu.step(&mut bus);
        assert_eq!(cpu.registers.f & FLAG_C, FLAG_C);
        assert_eq!(cpu.registers.f & FLAG_N, 0);
        assert_eq!(cpu.registers.f & FLAG_H, 0);

        cpu.step(&mut bus);
        assert_eq!(cpu.registers.f & FLAG_C, 0);
        assert_eq!(cpu.registers.f & FLAG_N, 0);
        assert_eq!(cpu.registers.f & FLAG_H, 0);
    }

    #[test]
    fn jump_call_ret_and_stack_opcodes_follow_control_flow() {
        let mut cpu = Cpu::new();
        cpu.registers.set_bc(0xBEEF);
        cpu.registers.f = FLAG_Z;
        let mut bus = make_bus_with_program(&[
            0x20, 0x02, // JR NZ,+2 (not taken because Z set)
            0x00, // NOP
            0xCD, 0x09, 0x00, // CALL 0009
            0xC3, 0x0C, 0x00, // JP 000C
            0xC5, // [0009] PUSH BC
            0xD1, // POP DE
            0xC9, // RET
            0x18, 0x02, // [000C] JR +2
            0x00, // skipped NOP
            0x00, // final NOP
        ]);

        cpu.step(&mut bus); // JR NZ,+2 (not taken)
        assert_eq!(cpu.pc(), 0x0002);

        cpu.step(&mut bus); // NOP
        cpu.step(&mut bus); // CALL 0008
        assert_eq!(cpu.pc(), 0x0009);

        cpu.step(&mut bus); // PUSH BC
        cpu.step(&mut bus); // POP DE
        assert_eq!(cpu.registers.de(), 0xBEEF);

        cpu.step(&mut bus); // RET
        assert_eq!(cpu.pc(), 0x0006);

        cpu.step(&mut bus); // JP 000C
        assert_eq!(cpu.pc(), 0x000C);

        cpu.step(&mut bus); // JR +2
        assert_eq!(cpu.pc(), 0x0010);
    }

    #[test]
    fn sp_offset_loads_set_flags_and_destinations() {
        let mut cpu = Cpu::new();
        cpu.sp = 0xFFF8;
        cpu.registers.f = FLAG_Z | FLAG_N;
        let mut bus = make_bus_with_program(&[
            0xE8, 0x08, // ADD SP,+8 => 0000, H and C set
            0xF8, 0xF8, // LD HL,SP-8 => FFF8
            0xF9, // LD SP,HL
            0x08, 0x00, 0xC1, // LD (C100),SP
        ]);

        cpu.step(&mut bus);
        assert_eq!(cpu.sp, 0x0000);
        assert_eq!(cpu.registers.f & FLAG_Z, 0);
        assert_eq!(cpu.registers.f & FLAG_N, 0);
        assert_eq!(cpu.registers.f & FLAG_H, FLAG_H);
        assert_eq!(cpu.registers.f & FLAG_C, FLAG_C);

        cpu.step(&mut bus);
        assert_eq!(cpu.registers.hl(), 0xFFF8);
        assert_eq!(cpu.registers.f & FLAG_H, 0);
        assert_eq!(cpu.registers.f & FLAG_C, 0);

        cpu.step(&mut bus);
        assert_eq!(cpu.sp, 0xFFF8);

        cpu.step(&mut bus);
        assert_eq!(bus.read8(0xC100), 0xF8);
        assert_eq!(bus.read8(0xC101), 0xFF);
    }

    #[test]
    fn cb_prefixed_bit_operations_cover_register_and_hl_paths() {
        let mut cpu = Cpu::new();
        cpu.registers.a = 0b1000_0001;
        cpu.registers.b = 0b1000_0000;
        cpu.registers.c = 0b0000_0001;
        cpu.registers.d = 0b1111_0000;
        cpu.registers.set_hl(0xC200);
        cpu.registers.f = FLAG_C;
        let mut bus = make_bus_with_program(&[
            0xCB, 0x07, // RLC A  => 0000_0011, C=1
            0xCB, 0x10, // RL B   => uses carry-in, becomes 0000_0001
            0xCB, 0x29, // SRA C  => 0000_0000, C=1, Z=1
            0xCB, 0x62, // BIT 4,D => clear, Z=0, H=1
            0xCB, 0xA2, // RES 4,D => 1110_0000
            0xCB, 0xEE, // SET 5,(HL) memory path
            0xCB, 0x46, // BIT 0,(HL) => set, Z=0 (12 cycles)
        ]);
        bus.write8(0xC200, 0b0000_0001);

        assert_eq!(cpu.step(&mut bus), 8);
        assert_eq!(cpu.registers.a, 0b0000_0011);
        assert_eq!(cpu.registers.f & FLAG_C, FLAG_C);

        assert_eq!(cpu.step(&mut bus), 8);
        assert_eq!(cpu.registers.b, 0b0000_0001);
        assert_eq!(cpu.registers.f & FLAG_C, FLAG_C);

        assert_eq!(cpu.step(&mut bus), 8);
        assert_eq!(cpu.registers.c, 0);
        assert_eq!(cpu.registers.f & FLAG_Z, FLAG_Z);
        assert_eq!(cpu.registers.f & FLAG_C, FLAG_C);

        assert_eq!(cpu.step(&mut bus), 8);
        assert_eq!(cpu.registers.f & FLAG_Z, 0);
        assert_eq!(cpu.registers.f & FLAG_H, FLAG_H);

        assert_eq!(cpu.step(&mut bus), 8);
        assert_eq!(cpu.registers.d, 0b1110_0000);

        assert_eq!(cpu.step(&mut bus), 16);
        assert_eq!(bus.read8(0xC200), 0b0010_0001);

        assert_eq!(cpu.step(&mut bus), 12);
        assert_eq!(cpu.registers.f & FLAG_Z, 0);
        assert_eq!(cpu.registers.f & FLAG_H, FLAG_H);
    }

    #[test]
    fn stop_di_ei_and_reti_update_cpu_interrupt_state() {
        let mut cpu = Cpu::new();
        cpu.sp = 0xFFFC;
        let mut bus = make_bus_with_program(&[
            0xFB, // EI (IME enabled after next instruction)
            0x00, // NOP (completes EI delay)
            0xF3, // DI
            0x10, 0x00, // STOP 00
        ]);
        bus.write8(0xFFFC, 0x34);
        bus.write8(0xFFFD, 0x12);

        assert!(!cpu.ime());
        assert_eq!(cpu.step(&mut bus), 4);
        assert!(!cpu.ime());

        assert_eq!(cpu.step(&mut bus), 4);
        assert!(cpu.ime());

        assert_eq!(cpu.step(&mut bus), 4);
        assert!(!cpu.ime());

        assert_eq!(cpu.step(&mut bus), 4);
        assert!(cpu.halted());
        assert_eq!(cpu.pc(), 0x0005);

        let mut cpu = Cpu::new();
        cpu.sp = 0xFFFC;
        let mut bus = make_bus_with_program(&[0xD9]); // RETI
        bus.write8(0xFFFC, 0x78);
        bus.write8(0xFFFD, 0x56);

        assert_eq!(cpu.step(&mut bus), 16);
        assert_eq!(cpu.pc(), 0x5678);
        assert_eq!(cpu.sp(), 0xFFFE);
        assert!(cpu.ime());
    }

    #[test]
    fn unimplemented_opcode_halts_without_panicking() {
        let mut cpu = Cpu::new();
        let mut bus = make_bus_with_program(&[0xD3]); // unused/unimplemented opcode

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 4);
        assert!(cpu.halted());
        assert_eq!(cpu.last_unimplemented_opcode(), Some(0xD3));
        assert_eq!(cpu.pc(), 0x0001);
    }
}
