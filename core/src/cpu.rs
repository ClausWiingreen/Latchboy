use crate::bus::Bus;

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
                self.registers.a = self.registers.a.wrapping_add(1);
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
