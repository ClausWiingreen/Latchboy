use crate::bus::Bus;
use crate::interrupts;
use bitflags::bitflags;

pub mod metadata;

use metadata::{Condition, Instruction, OpcodeMetadata, Operand16, Operand8};

bitflags! {
    /// Typed CPU `F` flag register; only the upper nibble is hardware-backed.
    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
    pub struct CpuFlags: u8 {
        const ZERO = 0b1000_0000;
        const SUBTRACT = 0b0100_0000;
        const HALF_CARRY = 0b0010_0000;
        const CARRY = 0b0001_0000;
    }
}

impl CpuFlags {
    pub const fn read_bits(self) -> u8 {
        self.bits()
    }

    pub fn write_bits(&mut self, value: u8) {
        *self = Self::from_bits_truncate(value);
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Registers {
    pub a: u8,
    pub f: CpuFlags,
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
    const fn bit(self) -> CpuFlags {
        match self {
            Self::Zero => CpuFlags::ZERO,
            Self::Subtract => CpuFlags::SUBTRACT,
            Self::HalfCarry => CpuFlags::HALF_CARRY,
            Self::Carry => CpuFlags::CARRY,
        }
    }
}

impl Registers {
    pub const fn af(&self) -> u16 {
        u16::from_be_bytes([self.a, self.f.read_bits()])
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
        self.f.write_bits(f);
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
            self.f.insert(flag.bit());
        } else {
            self.f.remove(flag.bit());
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Cpu {
    registers: Registers,
    pc: u16,
    sp: u16,
    halted: bool,
    halted_by_unimplemented_opcode: bool,
    ime: bool,
    ime_enable_pending: bool,
    halt_bug_active: bool,
    last_unimplemented_opcode: Option<u8>,
    last_step_fetch_bytes: [u8; 3],
    last_step_fetch_count: u8,
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
                f: CpuFlags::empty(),
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
            halted_by_unimplemented_opcode: false,
            ime: false,
            ime_enable_pending: false,
            halt_bug_active: false,
            last_unimplemented_opcode: None,
            last_step_fetch_bytes: [0; 3],
            last_step_fetch_count: 0,
        }
    }

    pub const fn new_dmg_no_boot() -> Self {
        Self {
            registers: Registers {
                a: 0x01,
                f: CpuFlags::from_bits_retain(0xB0),
                b: 0x00,
                c: 0x13,
                d: 0x00,
                e: 0xD8,
                h: 0x01,
                l: 0x4D,
            },
            pc: 0x0100,
            sp: 0xFFFE,
            halted: false,
            halted_by_unimplemented_opcode: false,
            ime: false,
            ime_enable_pending: false,
            halt_bug_active: false,
            last_unimplemented_opcode: None,
            last_step_fetch_bytes: [0; 3],
            last_step_fetch_count: 0,
        }
    }

    pub const fn new_cgb_no_boot() -> Self {
        Self {
            registers: Registers {
                a: 0x11,
                f: CpuFlags::from_bits_retain(0x80),
                b: 0x00,
                c: 0x00,
                d: 0xFF,
                e: 0x56,
                h: 0x00,
                l: 0x0D,
            },
            pc: 0x0100,
            sp: 0xFFFE,
            halted: false,
            halted_by_unimplemented_opcode: false,
            ime: false,
            ime_enable_pending: false,
            halt_bug_active: false,
            last_unimplemented_opcode: None,
            last_step_fetch_bytes: [0; 3],
            last_step_fetch_count: 0,
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

    pub const fn halted_is_interrupt_wakeable(&self) -> bool {
        self.halted && !self.halted_by_unimplemented_opcode
    }

    pub const fn ime(&self) -> bool {
        self.ime
    }

    pub const fn last_unimplemented_opcode(&self) -> Option<u8> {
        self.last_unimplemented_opcode
    }

    pub(crate) fn last_step_operand1_fetch(&self) -> Option<u8> {
        if self.last_step_fetch_count > 1 {
            Some(self.last_step_fetch_bytes[1])
        } else {
            None
        }
    }

    pub(crate) fn last_step_operand2_fetch(&self) -> Option<u8> {
        if self.last_step_fetch_count > 2 {
            Some(self.last_step_fetch_bytes[2])
        } else {
            None
        }
    }

    pub(crate) fn will_service_interrupt(&self, bus: &Bus) -> bool {
        let pending_interrupts = self.pending_interrupts(bus);
        pending_interrupts != 0 && !self.halted_by_unimplemented_opcode && self.ime
    }

    pub fn step(&mut self, bus: &mut Bus) -> u32 {
        self.last_step_fetch_count = 0;
        let pending_interrupts = self.pending_interrupts(bus);
        if pending_interrupts != 0 && !self.halted_by_unimplemented_opcode {
            if self.halted {
                self.halted = false;
            }
            if self.ime {
                return self.service_interrupt(bus, pending_interrupts);
            }
        }

        if self.halted {
            return 4;
        }

        let enable_ime_after_instruction = self.ime_enable_pending;
        self.ime_enable_pending = false;

        let opcode = self.fetch8(bus);
        let cycles = if let Some(metadata) = metadata::OPCODES[opcode as usize] {
            self.execute_metadata(
                metadata,
                bus,
                pending_interrupts,
                enable_ime_after_instruction,
            )
        } else {
            self.handle_unimplemented_opcode(opcode)
        };

        if enable_ime_after_instruction && opcode != 0xF3 {
            self.ime = true;
        }

        cycles
    }

    fn execute_metadata(
        &mut self,
        metadata: OpcodeMetadata,
        bus: &mut Bus,
        pending_interrupts: u8,
        enable_ime_after_instruction: bool,
    ) -> u32 {
        match metadata.instruction {
            Instruction::Nop => metadata.cycles.for_branch(false),
            Instruction::Rlca => {
                self.rlca();
                metadata.cycles.for_branch(false)
            }
            Instruction::Rrca => {
                self.rrca();
                metadata.cycles.for_branch(false)
            }
            Instruction::Rla => {
                self.rla();
                metadata.cycles.for_branch(false)
            }
            Instruction::Rra => {
                self.rra();
                metadata.cycles.for_branch(false)
            }
            Instruction::Daa => {
                self.daa();
                metadata.cycles.for_branch(false)
            }
            Instruction::Cpl => {
                self.registers.a = !self.registers.a;
                self.registers.set_flag(Flag::Subtract, true);
                self.registers.set_flag(Flag::HalfCarry, true);
                metadata.cycles.for_branch(false)
            }
            Instruction::Scf => {
                self.registers.set_flag(Flag::Subtract, false);
                self.registers.set_flag(Flag::HalfCarry, false);
                self.registers.set_flag(Flag::Carry, true);
                metadata.cycles.for_branch(false)
            }
            Instruction::Ccf => {
                let carry = !self.registers.f.contains(CpuFlags::CARRY);
                self.registers.set_flag(Flag::Subtract, false);
                self.registers.set_flag(Flag::HalfCarry, false);
                self.registers.set_flag(Flag::Carry, carry);
                metadata.cycles.for_branch(false)
            }
            Instruction::LdSpToImm16Addr => {
                let address =
                    self.fetch_operand16(metadata.operand16.expect("LD SP,(a16) operand"), bus);
                let [lo, hi] = self.sp.to_le_bytes();
                bus.write8(address, lo);
                bus.write8(address.wrapping_add(1), hi);
                metadata.cycles.for_branch(false)
            }
            Instruction::LdMemFromA => {
                let address = self.read_operand16(metadata.operand16.expect("LD (r16),A operand"));
                bus.write8(address, self.registers.a);
                metadata.cycles.for_branch(false)
            }
            Instruction::LdAFromMem => {
                let address = self.read_operand16(metadata.operand16.expect("LD A,(r16) operand"));
                self.registers.a = bus.read8(address);
                metadata.cycles.for_branch(false)
            }
            Instruction::Inc16 => {
                let operand = metadata.operand16.expect("INC r16 operand");
                let value = self.read_operand16(operand).wrapping_add(1);
                self.write_operand16(operand, value);
                metadata.cycles.for_branch(false)
            }
            Instruction::Dec16 => {
                let operand = metadata.operand16.expect("DEC r16 operand");
                let value = self.read_operand16(operand).wrapping_sub(1);
                self.write_operand16(operand, value);
                metadata.cycles.for_branch(false)
            }
            Instruction::Ld16Imm => {
                let operand = metadata.operand16.expect("LD r16,d16 operand");
                let value = self.fetch16(bus);
                self.write_operand16(operand, value);
                metadata.cycles.for_branch(false)
            }
            Instruction::AddHl => {
                let value = self.read_operand16(metadata.operand16.expect("ADD HL,r16 operand"));
                self.add_to_hl(value);
                metadata.cycles.for_branch(false)
            }
            Instruction::LdHliFromA => {
                let address = self.registers.hl();
                bus.write8(address, self.registers.a);
                self.registers.set_hl(address.wrapping_add(1));
                metadata.cycles.for_branch(false)
            }
            Instruction::LdAFromHli => {
                let address = self.registers.hl();
                self.registers.a = bus.read8(address);
                self.registers.set_hl(address.wrapping_add(1));
                metadata.cycles.for_branch(false)
            }
            Instruction::LdHldFromA => {
                let address = self.registers.hl();
                bus.write8(address, self.registers.a);
                self.registers.set_hl(address.wrapping_sub(1));
                metadata.cycles.for_branch(false)
            }
            Instruction::LdAFromHld => {
                let address = self.registers.hl();
                self.registers.a = bus.read8(address);
                self.registers.set_hl(address.wrapping_sub(1));
                metadata.cycles.for_branch(false)
            }
            Instruction::Stop => {
                let _ = self.fetch_operand8(metadata.operand8.expect("STOP padding operand"), bus);
                self.halted = !bus.consume_cgb_speed_switch_request();
                self.halted_by_unimplemented_opcode = false;
                metadata.cycles.for_branch(false)
            }
            Instruction::Jr => {
                let offset = self.fetch_operand8(metadata.operand8.expect("JR operand"), bus) as i8;
                self.pc = self.pc.wrapping_add_signed(i16::from(offset));
                metadata.cycles.for_branch(false)
            }
            Instruction::JrCond => {
                let offset =
                    self.fetch_operand8(metadata.operand8.expect("JR cc operand"), bus) as i8;
                let taken = self.condition_met_typed(metadata.condition.expect("JR condition"));
                if taken {
                    self.pc = self.pc.wrapping_add_signed(i16::from(offset));
                }
                metadata.cycles.for_branch(taken)
            }
            Instruction::Ld8Imm => {
                let target = metadata.operand8.expect("LD r8,d8 target");
                let value = self.fetch8(bus);
                self.write_operand8(target, value, bus);
                metadata.cycles.for_operand8(target)
            }
            Instruction::Inc8 => self.execute_inc8(metadata, bus),
            Instruction::Dec8 => self.execute_dec8(metadata, bus),
            Instruction::Halt => {
                self.execute_halt(pending_interrupts, enable_ime_after_instruction)
            }
            Instruction::Ld8 => {
                let source = match metadata.operand16.expect("LD r8,r8 source") {
                    Operand16::R8Source(index) => index,
                    _ => unreachable!("LD r8,r8 uses R8Source metadata"),
                };
                let target = metadata.operand8.expect("LD r8,r8 target");
                let value = self.read_r8(source, bus);
                self.write_operand8(target, value, bus);
                metadata.cycles.for_branch(false)
            }
            Instruction::Alu8 => self.execute_alu_metadata(metadata, bus),
            Instruction::AluImm8 => {
                let value = self.fetch_operand8(metadata.operand8.expect("ALU d8 operand"), bus);
                self.execute_alu_value(metadata.opcode, value);
                metadata.cycles.for_branch(false)
            }
            Instruction::RetCond => {
                let taken = self.condition_met_typed(metadata.condition.expect("RET condition"));
                if taken {
                    self.pc = self.pop_stack16(bus);
                }
                metadata.cycles.for_branch(taken)
            }
            Instruction::Pop => {
                let value = self.pop_stack16(bus);
                self.write_stack_operand16(metadata.operand16.expect("POP r16 operand"), value);
                metadata.cycles.for_branch(false)
            }
            Instruction::JpCond => {
                let address =
                    self.fetch_operand16(metadata.operand16.expect("JP cc,a16 operand"), bus);
                let taken = self.condition_met_typed(metadata.condition.expect("JP condition"));
                if taken {
                    self.pc = address;
                }
                metadata.cycles.for_branch(taken)
            }
            Instruction::Jp => {
                self.pc = self.fetch_operand16(metadata.operand16.expect("JP a16 operand"), bus);
                metadata.cycles.for_branch(false)
            }
            Instruction::CallCond => {
                let address =
                    self.fetch_operand16(metadata.operand16.expect("CALL cc,a16 operand"), bus);
                let taken = self.condition_met_typed(metadata.condition.expect("CALL condition"));
                if taken {
                    self.push_stack16(bus, self.pc);
                    self.pc = address;
                }
                metadata.cycles.for_branch(taken)
            }
            Instruction::Push => {
                let value =
                    self.read_stack_operand16(metadata.operand16.expect("PUSH r16 operand"));
                self.push_stack16(bus, value);
                metadata.cycles.for_branch(false)
            }
            Instruction::Rst => {
                let vector = match metadata.operand8.expect("RST vector operand") {
                    Operand8::Vector(vector) => u16::from(vector),
                    _ => unreachable!("RST uses vector metadata"),
                };
                self.push_stack16(bus, self.pc);
                self.pc = vector;
                metadata.cycles.for_branch(false)
            }
            Instruction::PrefixCb => {
                let cb_opcode = self.fetch_operand8(Operand8::CbOpcode, bus);
                self.execute_cb_metadata(metadata::CB_OPCODES[cb_opcode as usize], bus)
            }
            Instruction::Ret => {
                self.pc = self.pop_stack16(bus);
                metadata.cycles.for_branch(false)
            }
            Instruction::Call => {
                let address =
                    self.fetch_operand16(metadata.operand16.expect("CALL a16 operand"), bus);
                self.push_stack16(bus, self.pc);
                self.pc = address;
                metadata.cycles.for_branch(false)
            }
            Instruction::Reti => {
                self.pc = self.pop_stack16(bus);
                self.ime = true;
                self.ime_enable_pending = false;
                metadata.cycles.for_branch(false)
            }
            Instruction::AddSpE8 => {
                let offset =
                    self.fetch_operand8(metadata.operand8.expect("ADD SP,e8 operand"), bus) as i8;
                self.sp = self.add_signed_to_sp(offset);
                metadata.cycles.for_branch(false)
            }
            Instruction::JpHl => {
                self.pc = self.registers.hl();
                metadata.cycles.for_branch(false)
            }
            Instruction::LdhImmFromA => {
                let offset =
                    self.fetch_operand8(metadata.operand8.expect("LDH (a8),A operand"), bus);
                bus.write8(0xFF00u16 + u16::from(offset), self.registers.a);
                metadata.cycles.for_branch(false)
            }
            Instruction::LdhCFromA => {
                bus.write8(0xFF00u16 + u16::from(self.registers.c), self.registers.a);
                metadata.cycles.for_branch(false)
            }
            Instruction::LdImm16FromA => {
                let address =
                    self.fetch_operand16(metadata.operand16.expect("LD (a16),A operand"), bus);
                bus.write8(address, self.registers.a);
                metadata.cycles.for_branch(false)
            }
            Instruction::LdHlSpPlusE8 => {
                let offset =
                    self.fetch_operand8(metadata.operand8.expect("LD HL,SP+e8 operand"), bus) as i8;
                let result = self.add_signed_to_sp(offset);
                self.registers.set_hl(result);
                metadata.cycles.for_branch(false)
            }
            Instruction::LdSpHl => {
                self.sp = self.registers.hl();
                metadata.cycles.for_branch(false)
            }
            Instruction::LdhAFromImm => {
                let offset =
                    self.fetch_operand8(metadata.operand8.expect("LDH A,(a8) operand"), bus);
                self.registers.a = bus.read8(0xFF00u16 + u16::from(offset));
                metadata.cycles.for_branch(false)
            }
            Instruction::LdhAFromC => {
                self.registers.a = bus.read8(0xFF00u16 + u16::from(self.registers.c));
                metadata.cycles.for_branch(false)
            }
            Instruction::Di => {
                self.ime = false;
                self.ime_enable_pending = false;
                metadata.cycles.for_branch(false)
            }
            Instruction::LdAFromImm16 => {
                let address =
                    self.fetch_operand16(metadata.operand16.expect("LD A,(a16) operand"), bus);
                self.registers.a = bus.read8(address);
                metadata.cycles.for_branch(false)
            }
            Instruction::Ei => {
                self.ime_enable_pending = true;
                metadata.cycles.for_branch(false)
            }
            Instruction::CbRotate
            | Instruction::CbBit
            | Instruction::CbRes
            | Instruction::CbSet => self.execute_cb_metadata(metadata, bus),
        }
    }

    fn execute_halt(&mut self, pending_interrupts: u8, enable_ime_after_instruction: bool) -> u32 {
        if !self.ime && pending_interrupts != 0 {
            if enable_ime_after_instruction {
                self.pc = self.pc.wrapping_sub(1);
            } else {
                self.halt_bug_active = true;
            }
        } else {
            self.halted = true;
        }
        4
    }

    fn fetch_operand8(&mut self, operand: Operand8, bus: &Bus) -> u8 {
        match operand {
            Operand8::Imm8 | Operand8::SignedImm8 | Operand8::Relative | Operand8::CbOpcode => {
                self.fetch8(bus)
            }
            Operand8::R8(index) | Operand8::R8Value(index) => self.read_r8(index, bus),
            Operand8::Vector(vector) => vector,
        }
    }

    fn fetch_operand16(&mut self, operand: Operand16, bus: &Bus) -> u16 {
        match operand {
            Operand16::Imm16 => self.fetch16(bus),
            Operand16::Imm16Value(value) => value,
            Operand16::R16(index) => self.read_r16_by_index(index),
            Operand16::StackR16(index) => self.read_stack_r16_by_index(index),
            Operand16::R8Source(_) | Operand16::Bit(_) => {
                unreachable!("not a fetchable 16-bit operand")
            }
        }
    }

    fn read_operand16(&self, operand: Operand16) -> u16 {
        match operand {
            Operand16::R16(index) => self.read_r16_by_index(index),
            Operand16::StackR16(index) => self.read_stack_r16_by_index(index),
            Operand16::Imm16Value(value) => value,
            Operand16::Imm16 | Operand16::R8Source(_) | Operand16::Bit(_) => {
                unreachable!("operand is not readable without fetching")
            }
        }
    }

    fn write_operand16(&mut self, operand: Operand16, value: u16) {
        match operand {
            Operand16::R16(index) => self.write_r16_by_index(index, value),
            Operand16::StackR16(index) => self.write_stack_r16_by_index(index, value),
            Operand16::Imm16
            | Operand16::Imm16Value(_)
            | Operand16::R8Source(_)
            | Operand16::Bit(_) => unreachable!("operand is not writable"),
        }
    }

    fn write_operand8(&mut self, operand: Operand8, value: u8, bus: &mut Bus) {
        match operand {
            Operand8::R8(index) | Operand8::R8Value(index) => self.write_r8(index, value, bus),
            Operand8::Imm8
            | Operand8::SignedImm8
            | Operand8::Relative
            | Operand8::CbOpcode
            | Operand8::Vector(_) => unreachable!("operand is not writable"),
        }
    }

    fn condition_met_typed(&self, condition: Condition) -> bool {
        self.condition_met(condition.index())
    }

    fn read_stack_operand16(&self, operand: Operand16) -> u16 {
        match operand {
            Operand16::StackR16(index) => self.read_stack_r16_by_index(index),
            _ => unreachable!("stack operand expected"),
        }
    }

    fn write_stack_operand16(&mut self, operand: Operand16, value: u16) {
        match operand {
            Operand16::StackR16(index) => self.write_stack_r16_by_index(index, value),
            _ => unreachable!("stack operand expected"),
        }
    }

    fn read_stack_r16_by_index(&self, register_pair_index: u8) -> u16 {
        match register_pair_index & 0x03 {
            0x00 => self.registers.bc(),
            0x01 => self.registers.de(),
            0x02 => self.registers.hl(),
            0x03 => self.registers.af(),
            _ => unreachable!("register pair index is masked to 2 bits"),
        }
    }

    fn write_stack_r16_by_index(&mut self, register_pair_index: u8, value: u16) {
        match register_pair_index & 0x03 {
            0x00 => self.registers.set_bc(value),
            0x01 => self.registers.set_de(value),
            0x02 => self.registers.set_hl(value),
            0x03 => self.registers.set_af(value),
            _ => unreachable!("register pair index is masked to 2 bits"),
        }
    }

    fn execute_inc8(&mut self, metadata: OpcodeMetadata, bus: &mut Bus) -> u32 {
        let operand = metadata.operand8.expect("INC r8 operand");
        let register_index = match operand {
            Operand8::R8(index) => index,
            _ => unreachable!("INC uses R8"),
        };
        let previous = self.read_r8(register_index, bus);
        let result = previous.wrapping_add(1);
        self.write_r8(register_index, result, bus);
        self.registers.set_flag(Flag::Zero, result == 0);
        self.registers.set_flag(Flag::Subtract, false);
        self.registers
            .set_flag(Flag::HalfCarry, (previous & 0x0F) == 0x0F);
        metadata.cycles.for_operand8(operand)
    }

    fn execute_dec8(&mut self, metadata: OpcodeMetadata, bus: &mut Bus) -> u32 {
        let operand = metadata.operand8.expect("DEC r8 operand");
        let register_index = match operand {
            Operand8::R8(index) => index,
            _ => unreachable!("DEC uses R8"),
        };
        let previous = self.read_r8(register_index, bus);
        let result = previous.wrapping_sub(1);
        self.write_r8(register_index, result, bus);
        self.registers.set_flag(Flag::Zero, result == 0);
        self.registers.set_flag(Flag::Subtract, true);
        self.registers
            .set_flag(Flag::HalfCarry, (previous & 0x0F) == 0x00);
        metadata.cycles.for_operand8(operand)
    }

    fn execute_alu_metadata(&mut self, metadata: OpcodeMetadata, bus: &Bus) -> u32 {
        let operand = metadata.operand8.expect("ALU r8 operand");
        let value = self.fetch_operand8(operand, bus);
        self.execute_alu_value(metadata.opcode, value);
        metadata.cycles.for_operand8(operand)
    }

    fn execute_alu_value(&mut self, opcode: u8, value: u8) {
        match (opcode >> 3) & 0x07 {
            0x00 => self.add_to_a(value),
            0x01 => self.adc_to_a(value),
            0x02 => self.sub_from_a(value),
            0x03 => self.sbc_from_a(value),
            0x04 => self.and_with_a(value),
            0x05 => self.xor_with_a(value),
            0x06 => self.or_with_a(value),
            0x07 => self.compare_a(value),
            _ => unreachable!("alu operation index is masked to 3 bits"),
        }
    }

    fn execute_cb_metadata(&mut self, metadata: OpcodeMetadata, bus: &mut Bus) -> u32 {
        let register_index = match metadata.operand8.expect("CB r8 operand") {
            Operand8::R8(index) => index,
            _ => unreachable!("CB uses R8 operand"),
        };
        let bit_index = match metadata.operand16.expect("CB bit operand") {
            Operand16::Bit(bit) => bit,
            _ => unreachable!("CB uses bit metadata"),
        };

        match metadata.instruction {
            Instruction::CbRotate => self.execute_cb_rotate(register_index, bit_index, bus),
            Instruction::CbBit => {
                let value = self.read_r8(register_index, bus);
                self.registers
                    .set_flag(Flag::Zero, (value & (1 << bit_index)) == 0);
                self.registers.set_flag(Flag::Subtract, false);
                self.registers.set_flag(Flag::HalfCarry, true);
            }
            Instruction::CbRes => {
                let value = self.read_r8(register_index, bus) & !(1 << bit_index);
                self.write_r8(register_index, value, bus);
            }
            Instruction::CbSet => {
                let value = self.read_r8(register_index, bus) | (1 << bit_index);
                self.write_r8(register_index, value, bus);
            }
            _ => unreachable!("not a CB instruction"),
        }
        metadata.cycles.for_branch(false)
    }

    fn execute_cb_rotate(&mut self, register_index: u8, operation_index: u8, bus: &mut Bus) {
        let value = self.read_r8(register_index, bus);
        let (result, carry) = match operation_index {
            0x00 => (value.rotate_left(1), (value & 0x80) != 0),
            0x01 => (value.rotate_right(1), (value & 0x01) != 0),
            0x02 => {
                let carry_in = u8::from(self.registers.f.contains(CpuFlags::CARRY));
                ((value << 1) | carry_in, (value & 0x80) != 0)
            }
            0x03 => {
                let carry_in = if self.registers.f.contains(CpuFlags::CARRY) {
                    0x80
                } else {
                    0x00
                };
                ((value >> 1) | carry_in, (value & 0x01) != 0)
            }
            0x04 => (value << 1, (value & 0x80) != 0),
            0x05 => (((value >> 1) | (value & 0x80)), (value & 0x01) != 0),
            0x06 => (value.rotate_left(4), false),
            0x07 => (value >> 1, (value & 0x01) != 0),
            _ => unreachable!("bit index is masked to 3 bits"),
        };
        self.write_r8(register_index, result, bus);
        self.registers.set_flag(Flag::Zero, result == 0);
        self.registers.set_flag(Flag::Subtract, false);
        self.registers.set_flag(Flag::HalfCarry, false);
        self.registers.set_flag(Flag::Carry, carry);
    }

    fn pending_interrupts(&self, bus: &Bus) -> u8 {
        bus.interrupt_flag() & bus.interrupt_enable() & interrupts::MASK
    }

    fn service_interrupt(&mut self, bus: &mut Bus, pending_interrupts: u8) -> u32 {
        let interrupt_index = pending_interrupts.trailing_zeros() as u16;
        let interrupt_mask = 1 << interrupt_index;
        let vector = interrupts::VECTORS[interrupt_index as usize];

        bus.clear_interrupt_flag_bits(interrupt_mask as u8);

        self.ime = false;
        self.ime_enable_pending = false;
        self.push_stack16(bus, self.pc);
        self.pc = vector;

        20
    }

    fn handle_unimplemented_opcode(&mut self, opcode: u8) -> u32 {
        self.halted = true;
        self.halted_by_unimplemented_opcode = true;
        self.last_unimplemented_opcode = Some(opcode);
        4
    }

    fn fetch8(&mut self, bus: &Bus) -> u8 {
        let value = bus.read8(self.pc);
        if self.last_step_fetch_count < self.last_step_fetch_bytes.len() as u8 {
            let index = self.last_step_fetch_count as usize;
            self.last_step_fetch_bytes[index] = value;
            self.last_step_fetch_count += 1;
        }
        if self.halt_bug_active {
            self.halt_bug_active = false;
        } else {
            self.pc = self.pc.wrapping_add(1);
        }
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

    fn read_r16_by_index(&self, register_pair_index: u8) -> u16 {
        match register_pair_index & 0x03 {
            0x00 => self.registers.bc(),
            0x01 => self.registers.de(),
            0x02 => self.registers.hl(),
            0x03 => self.sp,
            _ => unreachable!("register pair index is masked to 2 bits"),
        }
    }

    fn write_r16_by_index(&mut self, register_pair_index: u8, value: u16) {
        match register_pair_index & 0x03 {
            0x00 => self.registers.set_bc(value),
            0x01 => self.registers.set_de(value),
            0x02 => self.registers.set_hl(value),
            0x03 => self.sp = value,
            _ => unreachable!("register pair index is masked to 2 bits"),
        }
    }

    fn condition_met(&self, condition_index: u8) -> bool {
        match condition_index & 0x03 {
            0x00 => !self.registers.f.contains(CpuFlags::ZERO),
            0x01 => self.registers.f.contains(CpuFlags::ZERO),
            0x02 => !self.registers.f.contains(CpuFlags::CARRY),
            0x03 => self.registers.f.contains(CpuFlags::CARRY),
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
        let carry_in = u8::from(self.registers.f.contains(CpuFlags::CARRY));
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
        let carry_in = u8::from(self.registers.f.contains(CpuFlags::CARRY));
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
        let carry_in = u8::from(self.registers.f.contains(CpuFlags::CARRY));
        let carry_out = (self.registers.a & 0x80) != 0;
        self.registers.a = (self.registers.a << 1) | carry_in;
        self.registers.set_flag(Flag::Zero, false);
        self.registers.set_flag(Flag::Subtract, false);
        self.registers.set_flag(Flag::HalfCarry, false);
        self.registers.set_flag(Flag::Carry, carry_out);
    }

    fn rra(&mut self) {
        let carry_in = if self.registers.f.contains(CpuFlags::CARRY) {
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

        if !self.registers.f.contains(CpuFlags::SUBTRACT) {
            if self.registers.f.contains(CpuFlags::HALF_CARRY) || (self.registers.a & 0x0F) > 0x09 {
                adjust |= 0x06;
            }
            if self.registers.f.contains(CpuFlags::CARRY) || self.registers.a > 0x99 {
                adjust |= 0x60;
                set_carry = true;
            }
            self.registers.a = self.registers.a.wrapping_add(adjust);
        } else {
            if self.registers.f.contains(CpuFlags::HALF_CARRY) {
                adjust |= 0x06;
            }
            if self.registers.f.contains(CpuFlags::CARRY) {
                adjust |= 0x60;
            }
            self.registers.a = self.registers.a.wrapping_sub(adjust);
            set_carry = self.registers.f.contains(CpuFlags::CARRY);
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

    fn make_cgb_bus_with_program(program: &[u8]) -> Bus {
        let mut rom = vec![0u8; 2 * 16 * 1024];
        rom[..program.len()].copy_from_slice(program);
        rom[0x0134..0x0138].copy_from_slice(b"CPUT");
        rom[0x0147] = CartridgeType::RomOnly.code();
        rom[0x0148] = RomSize::Banks2.code();
        rom[0x0149] = RamSize::None.code();
        rom[0x014A] = DestinationCode::Japanese.code();
        rom[0x014D] = compute_header_checksum(&rom).expect("header checksum should compute");

        let cartridge = Cartridge::from_rom(rom).expect("test rom should parse");
        Bus::new_cgb(cartridge)
    }

    fn run_program(cpu: &mut Cpu, bus: &mut Bus, steps: usize) {
        for _ in 0..steps {
            cpu.step(bus);
        }
    }

    fn metadata_cycles_for_program(cpu: &Cpu, program: &[u8]) -> u32 {
        let opcode = program[0];
        let metadata =
            metadata::OPCODES[opcode as usize].expect("opcode should be in metadata table");
        if metadata.instruction == Instruction::PrefixCb {
            let cb_opcode = program[1];
            return metadata::CB_OPCODES[cb_opcode as usize]
                .cycles
                .fixed()
                .expect("CB opcode cycles are fixed");
        }

        match metadata.cycles {
            metadata::CycleCost::Fixed(cycles) => cycles,
            metadata::CycleCost::Branch { .. } => {
                let taken = cpu.condition_met_typed(metadata.condition.expect("branch condition"));
                metadata.cycles.for_branch(taken)
            }
            metadata::CycleCost::MemoryOperand { .. } => metadata.cycles.for_operand8(
                metadata
                    .operand8
                    .expect("memory operand timing needs an 8-bit operand"),
            ),
        }
    }

    #[test]
    fn inc_a_sets_z_and_h_and_clears_n() {
        let mut cpu = Cpu::new();
        cpu.registers.a = 0xFF;
        cpu.registers.f = CpuFlags::CARRY | CpuFlags::SUBTRACT;
        let mut bus = make_bus_with_program(&[0x3C]); // INC A

        cpu.step(&mut bus);

        assert_eq!(cpu.registers.a, 0x00);
        assert_eq!(cpu.registers.f & CpuFlags::ZERO, CpuFlags::ZERO);
        assert_eq!(cpu.registers.f & CpuFlags::HALF_CARRY, CpuFlags::HALF_CARRY);
        assert_eq!(cpu.registers.f & CpuFlags::SUBTRACT, CpuFlags::empty());
        assert_eq!(cpu.registers.f & CpuFlags::CARRY, CpuFlags::CARRY);
    }

    #[test]
    fn inc_a_clears_z_when_result_non_zero() {
        let mut cpu = Cpu::new();
        cpu.registers.a = 0x0E;
        cpu.registers.f = CpuFlags::ZERO | CpuFlags::CARRY;
        let mut bus = make_bus_with_program(&[0x3C]); // INC A

        cpu.step(&mut bus);

        assert_eq!(cpu.registers.a, 0x0F);
        assert_eq!(cpu.registers.f & CpuFlags::ZERO, CpuFlags::empty());
        assert_eq!(cpu.registers.f & CpuFlags::HALF_CARRY, CpuFlags::empty());
        assert_eq!(cpu.registers.f & CpuFlags::CARRY, CpuFlags::CARRY);
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
        cpu.registers.f = CpuFlags::CARRY;
        let mut bus = make_bus_with_program(&[0x3C]); // INC A

        cpu.step(&mut bus);

        assert_eq!(cpu.registers.f & CpuFlags::ZERO, CpuFlags::ZERO);
        assert_eq!(cpu.registers.f & CpuFlags::SUBTRACT, CpuFlags::empty());
        assert_eq!(cpu.registers.f & CpuFlags::HALF_CARRY, CpuFlags::HALF_CARRY);
        assert_eq!(cpu.registers.f & CpuFlags::CARRY, CpuFlags::CARRY);
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
        assert_eq!(cpu.registers.f & CpuFlags::HALF_CARRY, CpuFlags::HALF_CARRY);
        assert_eq!(cpu.registers.f & CpuFlags::SUBTRACT, CpuFlags::empty());

        cpu.step(&mut bus);
        assert_eq!(cpu.registers.a, 0x00);
        assert_eq!(cpu.registers.f & CpuFlags::ZERO, CpuFlags::ZERO);
        assert_eq!(cpu.registers.f & CpuFlags::SUBTRACT, CpuFlags::SUBTRACT);

        cpu.step(&mut bus);
        assert_eq!(cpu.registers.a, 0x00);
        assert_eq!(cpu.registers.f & CpuFlags::HALF_CARRY, CpuFlags::HALF_CARRY);

        cpu.step(&mut bus);
        assert_eq!(cpu.registers.a, 0x10);

        cpu.step(&mut bus);
        assert_eq!(cpu.registers.a, 0x11);

        cpu.step(&mut bus);
        assert_eq!(cpu.registers.a, 0x11);
        assert_eq!(cpu.registers.f & CpuFlags::CARRY, CpuFlags::empty());
        assert_eq!(cpu.registers.f & CpuFlags::ZERO, CpuFlags::empty());
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
        cpu.registers.f = CpuFlags::CARRY;
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
        assert_eq!(cpu.registers.f & CpuFlags::HALF_CARRY, CpuFlags::HALF_CARRY);
        assert_eq!(cpu.registers.f & CpuFlags::CARRY, CpuFlags::empty());

        cpu.step(&mut bus);
        assert_eq!(cpu.registers.a, 0xFF);

        cpu.step(&mut bus);
        assert_eq!(cpu.registers.a, 0x0F);
        assert_eq!(cpu.registers.f & CpuFlags::SUBTRACT, CpuFlags::SUBTRACT);

        cpu.step(&mut bus);
        assert_eq!(cpu.registers.a, 0x01);

        cpu.step(&mut bus);
        assert_eq!(cpu.registers.a, 0x01);
        assert_eq!(cpu.registers.f & CpuFlags::HALF_CARRY, CpuFlags::HALF_CARRY);

        cpu.step(&mut bus);
        assert_eq!(cpu.registers.a, 0x00);
        assert_eq!(cpu.registers.f & CpuFlags::ZERO, CpuFlags::ZERO);

        cpu.step(&mut bus);
        assert_eq!(cpu.registers.a, 0x80);

        cpu.step(&mut bus);
        assert_eq!(cpu.registers.a, 0x80);
        assert_eq!(cpu.registers.f & CpuFlags::ZERO, CpuFlags::ZERO);
    }

    #[test]
    fn inc_and_dec_registers_preserve_or_update_flags_like_hardware() {
        let mut cpu = Cpu::new();
        cpu.registers.b = 0x0F;
        cpu.registers.c = 0x00;
        cpu.registers.f = CpuFlags::CARRY;
        let mut bus = make_bus_with_program(&[
            0x04, // INC B -> 10, H set, C preserved
            0x0D, // DEC C -> FF, H set, N set
        ]);

        cpu.step(&mut bus);
        assert_eq!(cpu.registers.b, 0x10);
        assert_eq!(cpu.registers.f & CpuFlags::HALF_CARRY, CpuFlags::HALF_CARRY);
        assert_eq!(cpu.registers.f & CpuFlags::CARRY, CpuFlags::CARRY);
        assert_eq!(cpu.registers.f & CpuFlags::SUBTRACT, CpuFlags::empty());

        cpu.step(&mut bus);
        assert_eq!(cpu.registers.c, 0xFF);
        assert_eq!(cpu.registers.f & CpuFlags::SUBTRACT, CpuFlags::SUBTRACT);
        assert_eq!(cpu.registers.f & CpuFlags::HALF_CARRY, CpuFlags::HALF_CARRY);
        assert_eq!(cpu.registers.f & CpuFlags::CARRY, CpuFlags::CARRY);
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
        cpu.registers.f = CpuFlags::ZERO;
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
        assert_eq!(cpu.registers.f & CpuFlags::ZERO, CpuFlags::ZERO);
        assert_eq!(cpu.registers.f & CpuFlags::SUBTRACT, CpuFlags::empty());
        assert_eq!(cpu.registers.f & CpuFlags::HALF_CARRY, CpuFlags::HALF_CARRY);
        assert_eq!(cpu.registers.f & CpuFlags::CARRY, CpuFlags::CARRY);
    }

    #[test]
    fn accumulator_rotate_opcodes_use_expected_carry_paths() {
        let mut cpu = Cpu::new();
        cpu.registers.a = 0x85;
        cpu.registers.f = CpuFlags::ZERO;
        let mut bus = make_bus_with_program(&[
            0x07, // RLCA: 85 -> 0B, C=1
            0x0F, // RRCA: 0B -> 85, C=1
            0x17, // RLA: carry-in 1, 85 -> 0B, C=1
            0x1F, // RRA: carry-in 1, 0B -> 85, C=1
        ]);

        cpu.step(&mut bus);
        assert_eq!(cpu.registers.a, 0x0B);
        assert_eq!(cpu.registers.f & CpuFlags::CARRY, CpuFlags::CARRY);
        assert_eq!(cpu.registers.f & CpuFlags::ZERO, CpuFlags::empty());

        cpu.step(&mut bus);
        assert_eq!(cpu.registers.a, 0x85);
        assert_eq!(cpu.registers.f & CpuFlags::CARRY, CpuFlags::CARRY);

        cpu.step(&mut bus);
        assert_eq!(cpu.registers.a, 0x0B);
        assert_eq!(cpu.registers.f & CpuFlags::CARRY, CpuFlags::CARRY);

        cpu.step(&mut bus);
        assert_eq!(cpu.registers.a, 0x85);
        assert_eq!(cpu.registers.f & CpuFlags::CARRY, CpuFlags::CARRY);
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
        assert_eq!(cpu.registers.f & CpuFlags::ZERO, CpuFlags::ZERO);
        assert_eq!(cpu.registers.f & CpuFlags::CARRY, CpuFlags::CARRY);
        assert_eq!(cpu.registers.f & CpuFlags::HALF_CARRY, CpuFlags::empty());

        cpu.step(&mut bus);
        assert_eq!(cpu.registers.a, 0xFF);
        assert_eq!(cpu.registers.f & CpuFlags::SUBTRACT, CpuFlags::SUBTRACT);
        assert_eq!(cpu.registers.f & CpuFlags::HALF_CARRY, CpuFlags::HALF_CARRY);

        cpu.step(&mut bus);
        assert_eq!(cpu.registers.f & CpuFlags::CARRY, CpuFlags::CARRY);
        assert_eq!(cpu.registers.f & CpuFlags::SUBTRACT, CpuFlags::empty());
        assert_eq!(cpu.registers.f & CpuFlags::HALF_CARRY, CpuFlags::empty());

        cpu.step(&mut bus);
        assert_eq!(cpu.registers.f & CpuFlags::CARRY, CpuFlags::empty());
        assert_eq!(cpu.registers.f & CpuFlags::SUBTRACT, CpuFlags::empty());
        assert_eq!(cpu.registers.f & CpuFlags::HALF_CARRY, CpuFlags::empty());
    }

    #[test]
    fn jump_call_ret_and_stack_opcodes_follow_control_flow() {
        let mut cpu = Cpu::new();
        cpu.registers.set_bc(0xBEEF);
        cpu.registers.f = CpuFlags::ZERO;
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
        cpu.registers.f = CpuFlags::ZERO | CpuFlags::SUBTRACT;
        let mut bus = make_bus_with_program(&[
            0xE8, 0x08, // ADD SP,+8 => 0000, H and C set
            0xF8, 0xF8, // LD HL,SP-8 => FFF8
            0xF9, // LD SP,HL
            0x08, 0x00, 0xC1, // LD (C100),SP
        ]);

        cpu.step(&mut bus);
        assert_eq!(cpu.sp, 0x0000);
        assert_eq!(cpu.registers.f & CpuFlags::ZERO, CpuFlags::empty());
        assert_eq!(cpu.registers.f & CpuFlags::SUBTRACT, CpuFlags::empty());
        assert_eq!(cpu.registers.f & CpuFlags::HALF_CARRY, CpuFlags::HALF_CARRY);
        assert_eq!(cpu.registers.f & CpuFlags::CARRY, CpuFlags::CARRY);

        cpu.step(&mut bus);
        assert_eq!(cpu.registers.hl(), 0xFFF8);
        assert_eq!(cpu.registers.f & CpuFlags::HALF_CARRY, CpuFlags::empty());
        assert_eq!(cpu.registers.f & CpuFlags::CARRY, CpuFlags::empty());

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
        cpu.registers.f = CpuFlags::CARRY;
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
        assert_eq!(cpu.registers.f & CpuFlags::CARRY, CpuFlags::CARRY);

        assert_eq!(cpu.step(&mut bus), 8);
        assert_eq!(cpu.registers.b, 0b0000_0001);
        assert_eq!(cpu.registers.f & CpuFlags::CARRY, CpuFlags::CARRY);

        assert_eq!(cpu.step(&mut bus), 8);
        assert_eq!(cpu.registers.c, 0);
        assert_eq!(cpu.registers.f & CpuFlags::ZERO, CpuFlags::ZERO);
        assert_eq!(cpu.registers.f & CpuFlags::CARRY, CpuFlags::CARRY);

        assert_eq!(cpu.step(&mut bus), 8);
        assert_eq!(cpu.registers.f & CpuFlags::ZERO, CpuFlags::empty());
        assert_eq!(cpu.registers.f & CpuFlags::HALF_CARRY, CpuFlags::HALF_CARRY);

        assert_eq!(cpu.step(&mut bus), 8);
        assert_eq!(cpu.registers.d, 0b1110_0000);

        assert_eq!(cpu.step(&mut bus), 16);
        assert_eq!(bus.read8(0xC200), 0b0010_0001);

        assert_eq!(cpu.step(&mut bus), 12);
        assert_eq!(cpu.registers.f & CpuFlags::ZERO, CpuFlags::empty());
        assert_eq!(cpu.registers.f & CpuFlags::HALF_CARRY, CpuFlags::HALF_CARRY);
    }

    #[test]
    fn stop_ignores_key1_prepare_and_halts_in_dmg_mode() {
        let mut cpu = Cpu::new();
        let mut bus = make_bus_with_program(&[
            0x10, 0x00, // STOP 00 must remain a stop in DMG mode
            0x00,
        ]);
        bus.write8(0xFF4D, 0x01);

        assert_eq!(cpu.step(&mut bus), 4);

        assert!(cpu.halted());
        assert!(!bus.cgb_double_speed());
        assert_eq!(bus.read8(0xFF4D), 0xFF);
        assert_eq!(cpu.pc(), 0x0002);
    }

    #[test]
    fn stop_with_key1_prepare_toggles_cgb_double_speed_without_halting() {
        let mut cpu = Cpu::new();
        let mut bus = make_cgb_bus_with_program(&[
            0x10, 0x00, // STOP 00 consumes the pending KEY1 speed switch
            0x00, // NOP proves execution can continue after the switch
        ]);
        bus.write8(0xFF4D, 0x01);

        assert_eq!(cpu.step(&mut bus), 4);

        assert!(!cpu.halted());
        assert!(bus.cgb_double_speed());
        assert_eq!(bus.read8(0xFF4D), 0xFE);
        assert_eq!(cpu.pc(), 0x0002);

        assert_eq!(cpu.step(&mut bus), 4);
        assert_eq!(cpu.pc(), 0x0003);
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

        let mut cpu = Cpu::new();
        let mut bus = make_bus_with_program(&[
            0xFB, // EI
            0xF3, // DI (must cancel delayed EI effect)
            0x00, // NOP
        ]);

        assert_eq!(cpu.step(&mut bus), 4);
        assert!(!cpu.ime());

        assert_eq!(cpu.step(&mut bus), 4);
        assert!(!cpu.ime());

        assert_eq!(cpu.step(&mut bus), 4);
        assert!(!cpu.ime());
    }

    #[test]
    fn pending_enabled_interrupt_is_serviced_before_opcode_fetch() {
        let mut cpu = Cpu::new();
        let mut bus = make_bus_with_program(&[0x00]); // NOP (must not execute)

        cpu.pc = 0x1234;
        cpu.ime = true;
        bus.write8(0xFFFF, 0x01);
        bus.write8(0xFF0F, 0x01);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 20);
        assert_eq!(cpu.pc(), 0x0040);
        assert_eq!(cpu.sp(), 0xFFFC);
        assert!(!cpu.ime());
        assert_eq!(bus.read8(0xFF0F), 0x00);
        assert_eq!(bus.read8(0xFFFC), 0x34);
        assert_eq!(bus.read8(0xFFFD), 0x12);
    }

    #[test]
    fn interrupt_service_uses_hardware_priority_order() {
        let mut cpu = Cpu::new();
        let mut bus = make_bus_with_program(&[0x00]); // NOP (must not execute)

        cpu.pc = 0x3000;
        cpu.ime = true;
        bus.write8(0xFFFF, 0b0001_1111);
        bus.write8(0xFF0F, 0b0001_1000);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 20);
        assert_eq!(cpu.pc(), 0x0058);
        assert_eq!(bus.read8(0xFF0F), 0b0001_0000);
    }

    #[test]
    fn halted_cpu_wakes_on_pending_interrupt_even_when_ime_is_disabled() {
        let mut cpu = Cpu::new();
        let mut bus = make_bus_with_program(&[0x00]); // NOP

        cpu.halted = true;
        cpu.pc = 0x0000;
        bus.write8(0xFFFF, 0x01);
        bus.write8(0xFF0F, 0x01);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 4);
        assert!(!cpu.halted());
        assert_eq!(cpu.pc(), 0x0001);
        assert_eq!(bus.read8(0xFF0F), 0x01);
    }

    #[test]
    fn halt_bug_repeats_next_opcode_fetch_when_ime_is_disabled_with_pending_interrupt() {
        let mut cpu = Cpu::new();
        let mut bus = make_bus_with_program(&[
            0x76, // HALT
            0x3E, 0x12, // LD A,12
        ]);

        bus.write8(0xFFFF, 0x01);
        bus.write8(0xFF0F, 0x01);

        assert_eq!(cpu.step(&mut bus), 4);
        assert!(!cpu.halted());
        assert_eq!(cpu.pc(), 0x0001);

        assert_eq!(cpu.step(&mut bus), 8);
        assert_eq!(cpu.registers.a, 0x3E);
        assert_eq!(cpu.pc(), 0x0002);
    }

    #[test]
    fn ei_halt_bug_interrupt_returns_to_halt_instruction() {
        let mut cpu = Cpu::new();
        let mut program = vec![0x00; 0x41];
        program[0x0000] = 0xFB; // EI
        program[0x0001] = 0x76; // HALT
        program[0x0002] = 0x00; // NOP (must not execute before interrupt)
        program[0x0040] = 0xD9; // RETI
        let mut bus = make_bus_with_program(&program);

        bus.write8(0xFFFF, 0x01);
        bus.write8(0xFF0F, 0x01);

        assert_eq!(cpu.step(&mut bus), 4);
        assert!(!cpu.ime());
        assert_eq!(cpu.pc(), 0x0001);

        assert_eq!(cpu.step(&mut bus), 4);
        assert!(cpu.ime());
        assert!(!cpu.halted());
        assert_eq!(cpu.pc(), 0x0001);

        assert_eq!(cpu.step(&mut bus), 20);
        assert_eq!(cpu.pc(), 0x0040);
        assert_eq!(bus.read8(0xFFFC), 0x01);
        assert_eq!(bus.read8(0xFFFD), 0x00);

        assert_eq!(cpu.step(&mut bus), 16);
        assert_eq!(cpu.pc(), 0x0001);
        assert!(cpu.ime());

        assert_eq!(cpu.step(&mut bus), 4);
        assert!(cpu.halted());
        assert_eq!(cpu.pc(), 0x0002);
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

    #[test]
    fn unimplemented_opcode_trap_does_not_wake_on_pending_interrupts() {
        let mut cpu = Cpu::new();
        let mut bus = make_bus_with_program(&[0xD3]); // unused/unimplemented opcode

        assert_eq!(cpu.step(&mut bus), 4);
        assert!(cpu.halted());
        assert_eq!(cpu.last_unimplemented_opcode(), Some(0xD3));

        bus.write8(0xFF0F, 0x01);
        bus.write8(0xFFFF, 0x01);

        let cycles = cpu.step(&mut bus);
        assert_eq!(cycles, 4);
        assert!(cpu.halted());
        assert_eq!(cpu.pc(), 0x0001);
        assert_eq!(cpu.last_unimplemented_opcode(), Some(0xD3));
    }

    #[test]
    fn interrupt_service_clears_if_even_while_oam_dma_blocks_cpu_bus_access() {
        let mut cpu = Cpu::new();
        let mut bus = make_bus_with_program(&[0x00]); // NOP (must not execute)

        cpu.pc = 0x1234;
        cpu.ime = true;
        bus.write8(interrupts::ENABLE_REGISTER, 0x01);
        bus.write8(interrupts::FLAG_REGISTER, 0x01);
        bus.write8(crate::ppu::DMA_REGISTER, 0xC0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 20);
        assert_eq!(cpu.pc(), 0x0040);
        assert_eq!(cpu.sp(), 0xFFFC);
        assert_eq!(bus.interrupt_flag() & 0x01, 0x00);
    }

    #[test]
    fn table_driven_arithmetic_cases_match_expected_results() {
        struct Case {
            name: &'static str,
            program: &'static [u8],
            initial_a: u8,
            initial_b: u8,
            initial_flags: CpuFlags,
            expected_a: u8,
            expected_flags: CpuFlags,
        }

        let cases = [
            Case {
                name: "add_sets_half_carry_without_full_carry",
                program: &[0x80], // ADD A,B
                initial_a: 0x0F,
                initial_b: 0x01,
                initial_flags: CpuFlags::empty(),
                expected_a: 0x10,
                expected_flags: CpuFlags::HALF_CARRY,
            },
            Case {
                name: "adc_uses_carry_in",
                program: &[0x88], // ADC A,B
                initial_a: 0x7F,
                initial_b: 0x00,
                initial_flags: CpuFlags::CARRY,
                expected_a: 0x80,
                expected_flags: CpuFlags::HALF_CARRY,
            },
            Case {
                name: "sub_sets_subtract_and_zero",
                program: &[0x90], // SUB B
                initial_a: 0x22,
                initial_b: 0x22,
                initial_flags: CpuFlags::empty(),
                expected_a: 0x00,
                expected_flags: CpuFlags::ZERO | CpuFlags::SUBTRACT,
            },
            Case {
                name: "cp_updates_flags_but_not_accumulator",
                program: &[0xB8], // CP B
                initial_a: 0x20,
                initial_b: 0x30,
                initial_flags: CpuFlags::empty(),
                expected_a: 0x20,
                expected_flags: CpuFlags::SUBTRACT | CpuFlags::CARRY,
            },
        ];

        for case in cases {
            let mut cpu = Cpu::new();
            cpu.registers.a = case.initial_a;
            cpu.registers.b = case.initial_b;
            cpu.registers.f = case.initial_flags;
            let mut bus = make_bus_with_program(case.program);

            run_program(&mut cpu, &mut bus, 1);

            assert_eq!(cpu.registers.a, case.expected_a, "case: {}", case.name);
            assert_eq!(cpu.registers.f, case.expected_flags, "case: {}", case.name);
        }
    }

    #[test]
    fn table_driven_load_cases_cover_register_indirect_and_immediate_paths() {
        struct Case {
            program: &'static [u8],
            setup: fn(&mut Cpu, &mut Bus),
            assert_after: fn(&Cpu, &Bus),
            steps: usize,
        }

        let cases = [
            Case {
                program: &[0x06, 0xAB, 0x78], // LD B,AB; LD A,B
                setup: |_, _| {},
                assert_after: |cpu, _| {
                    assert_eq!(cpu.registers.b, 0xAB);
                    assert_eq!(cpu.registers.a, 0xAB);
                },
                steps: 2,
            },
            Case {
                program: &[0x22], // LD (HL+),A
                setup: |cpu, _| {
                    cpu.registers.a = 0x42;
                    cpu.registers.set_hl(0xC222);
                },
                assert_after: |cpu, bus| {
                    assert_eq!(bus.read8(0xC222), 0x42);
                    assert_eq!(cpu.registers.hl(), 0xC223);
                },
                steps: 1,
            },
            Case {
                program: &[0xE0, 0x80, 0x3E, 0x00, 0xF0, 0x80], // LDH (80),A; LD A,00; LDH A,(80)
                setup: |cpu, _| cpu.registers.a = 0x91,
                assert_after: |cpu, bus| {
                    assert_eq!(bus.read8(0xFF80), 0x91);
                    assert_eq!(cpu.registers.a, 0x91);
                },
                steps: 3,
            },
        ];

        for case in cases {
            let mut cpu = Cpu::new();
            let mut bus = make_bus_with_program(case.program);
            (case.setup)(&mut cpu, &mut bus);

            run_program(&mut cpu, &mut bus, case.steps);

            (case.assert_after)(&cpu, &bus);
        }
    }

    #[test]
    fn table_driven_cb_bitop_cases_cover_rotate_bit_res_and_set() {
        struct Case {
            name: &'static str,
            cb_opcode: u8,
            setup: fn(&mut Cpu, &mut Bus),
            assert_after: fn(&Cpu, &Bus),
            expected_flags: CpuFlags,
        }

        let cases = [
            Case {
                name: "rlc_b_rotates_bit7_into_carry",
                cb_opcode: 0x00, // RLC B
                setup: |cpu, _| cpu.registers.b = 0x81,
                assert_after: |cpu, _| assert_eq!(cpu.registers.b, 0x03),
                expected_flags: CpuFlags::CARRY,
            },
            Case {
                name: "bit_7_h_sets_zero_when_bit_clear",
                cb_opcode: 0x7C, // BIT 7,H
                setup: |cpu, _| cpu.registers.h = 0x7F,
                assert_after: |_, _| {},
                expected_flags: CpuFlags::ZERO | CpuFlags::HALF_CARRY,
            },
            Case {
                name: "res_4_d_clears_target_bit",
                cb_opcode: 0xA2, // RES 4,D
                setup: |cpu, _| cpu.registers.d = 0xFF,
                assert_after: |cpu, _| assert_eq!(cpu.registers.d, 0xEF),
                expected_flags: CpuFlags::empty(),
            },
            Case {
                name: "set_5_hl_writes_memory_path",
                cb_opcode: 0xEE, // SET 5,(HL)
                setup: |cpu, bus| {
                    cpu.registers.set_hl(0xC300);
                    bus.write8(0xC300, 0x01);
                    cpu.registers.f = CpuFlags::CARRY;
                },
                assert_after: |_, bus| assert_eq!(bus.read8(0xC300), 0x21),
                expected_flags: CpuFlags::CARRY,
            },
        ];

        for case in cases {
            let mut cpu = Cpu::new();
            let mut bus = make_bus_with_program(&[0xCB, case.cb_opcode]);
            (case.setup)(&mut cpu, &mut bus);

            let expected_cycles = metadata_cycles_for_program(&cpu, &[0xCB, case.cb_opcode]);
            let cycles = cpu.step(&mut bus);

            assert_eq!(cycles, expected_cycles, "case: {}", case.name);
            (case.assert_after)(&cpu, &bus);
            assert_eq!(cpu.registers.f, case.expected_flags, "case: {}", case.name);
        }
    }

    #[test]
    fn table_driven_instruction_cycle_counts_cover_branch_and_memory_paths() {
        struct Case {
            name: &'static str,
            program: &'static [u8],
            setup: fn(&mut Cpu, &mut Bus),
            expected_pc: u16,
        }

        let cases = [
            Case {
                name: "nop",
                program: &[0x00],
                setup: |_, _| {},
                expected_pc: 0x0001,
            },
            Case {
                name: "jr_taken",
                program: &[0x18, 0x02],
                setup: |_, _| {},
                expected_pc: 0x0004,
            },
            Case {
                name: "jr_nz_not_taken",
                program: &[0x20, 0x02],
                setup: |cpu, _| cpu.registers.f = CpuFlags::ZERO,
                expected_pc: 0x0002,
            },
            Case {
                name: "jr_nz_taken",
                program: &[0x20, 0x02],
                setup: |cpu, _| cpu.registers.f = CpuFlags::empty(),
                expected_pc: 0x0004,
            },
            Case {
                name: "ld_hl_d8_memory_path",
                program: &[0x36, 0x5A],
                setup: |cpu, _| cpu.registers.set_hl(0xC000),
                expected_pc: 0x0002,
            },
            Case {
                name: "ld_b_c_register_path",
                program: &[0x41],
                setup: |cpu, _| cpu.registers.c = 0x99,
                expected_pc: 0x0001,
            },
            Case {
                name: "ld_hl_b_memory_destination",
                program: &[0x70],
                setup: |cpu, _| {
                    cpu.registers.b = 0x33;
                    cpu.registers.set_hl(0xC123);
                },
                expected_pc: 0x0001,
            },
            Case {
                name: "ret_nz_not_taken",
                program: &[0xC0],
                setup: |cpu, _| cpu.registers.f = CpuFlags::ZERO,
                expected_pc: 0x0001,
            },
            Case {
                name: "ret_nz_taken",
                program: &[0xC0],
                setup: |cpu, bus| {
                    cpu.sp = 0xFFFC;
                    cpu.registers.f = CpuFlags::empty();
                    bus.write8(0xFFFC, 0x34);
                    bus.write8(0xFFFD, 0x12);
                },
                expected_pc: 0x1234,
            },
            Case {
                name: "cb_bit_hl",
                program: &[0xCB, 0x46],
                setup: |cpu, bus| {
                    cpu.registers.set_hl(0xC222);
                    bus.write8(0xC222, 0x01);
                },
                expected_pc: 0x0002,
            },
        ];

        for case in cases {
            let mut cpu = Cpu::new();
            let mut bus = make_bus_with_program(case.program);
            (case.setup)(&mut cpu, &mut bus);

            let expected_cycles = metadata_cycles_for_program(&cpu, case.program);
            let cycles = cpu.step(&mut bus);

            assert_eq!(cycles, expected_cycles, "case: {}", case.name);
            assert_eq!(cpu.pc(), case.expected_pc, "case: {}", case.name);
        }
    }

    #[test]
    fn memory_operand_paths_include_expected_timing_penalty() {
        struct Case {
            name: &'static str,
            program: &'static [u8],
            setup: fn(&mut Cpu, &mut Bus),
        }

        let cases = [
            Case {
                name: "add_a_b_register_path",
                program: &[0x80], // ADD A,B
                setup: |cpu, _| {
                    cpu.registers.a = 0x01;
                    cpu.registers.b = 0x02;
                },
            },
            Case {
                name: "add_a_hl_memory_path",
                program: &[0x86], // ADD A,(HL)
                setup: |cpu, bus| {
                    cpu.registers.a = 0x01;
                    cpu.registers.set_hl(0xC300);
                    bus.write8(0xC300, 0x02);
                },
            },
            Case {
                name: "cb_rlc_b_register_path",
                program: &[0xCB, 0x00], // RLC B
                setup: |cpu, _| cpu.registers.b = 0x81,
            },
            Case {
                name: "cb_rlc_hl_memory_path",
                program: &[0xCB, 0x06], // RLC (HL)
                setup: |cpu, bus| {
                    cpu.registers.set_hl(0xC301);
                    bus.write8(0xC301, 0x81);
                },
            },
        ];

        for case in cases {
            let mut cpu = Cpu::new();
            let mut bus = make_bus_with_program(case.program);
            (case.setup)(&mut cpu, &mut bus);

            let expected_cycles = metadata_cycles_for_program(&cpu, case.program);
            let cycles = cpu.step(&mut bus);

            assert_eq!(cycles, expected_cycles, "case: {}", case.name);
        }
    }
}
