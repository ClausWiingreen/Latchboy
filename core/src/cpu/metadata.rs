#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OpcodeMetadata {
    pub opcode: u8,
    pub instruction: Instruction,
    pub operand8: Option<Operand8>,
    pub operand16: Option<Operand16>,
    pub condition: Option<Condition>,
    pub cycles: CycleCost,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Instruction {
    Nop,
    Stop,
    Halt,
    PrefixCb,
    Ld8,
    Ld8Imm,
    Ld16Imm,
    LdSpToImm16Addr,
    LdMemFromA,
    LdAFromMem,
    LdHliFromA,
    LdAFromHli,
    LdHldFromA,
    LdAFromHld,
    LdhImmFromA,
    LdhCFromA,
    LdImm16FromA,
    LdhAFromImm,
    LdhAFromC,
    LdAFromImm16,
    Inc8,
    Dec8,
    Inc16,
    Dec16,
    AddHl,
    AddSpE8,
    LdHlSpPlusE8,
    LdSpHl,
    Alu8,
    AluImm8,
    Rlca,
    Rrca,
    Rla,
    Rra,
    Daa,
    Cpl,
    Scf,
    Ccf,
    Jr,
    JrCond,
    Jp,
    JpCond,
    JpHl,
    Call,
    CallCond,
    Ret,
    RetCond,
    Reti,
    Rst,
    Push,
    Pop,
    Di,
    Ei,
    CbRotate,
    CbBit,
    CbRes,
    CbSet,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Operand8 {
    R8(u8),
    R8Value(u8),
    Imm8,
    SignedImm8,
    Relative,
    CbOpcode,
    Vector(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Operand16 {
    R16(u8),
    StackR16(u8),
    R8Source(u8),
    Imm16,
    Imm16Value(u16),
    Bit(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Condition {
    Nz,
    Z,
    Nc,
    C,
}

impl Condition {
    pub const fn from_index(index: u8) -> Self {
        match index & 0x03 {
            0x00 => Self::Nz,
            0x01 => Self::Z,
            0x02 => Self::Nc,
            0x03 => Self::C,
            _ => unreachable!(),
        }
    }

    pub const fn index(self) -> u8 {
        match self {
            Self::Nz => 0,
            Self::Z => 1,
            Self::Nc => 2,
            Self::C => 3,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CycleCost {
    Fixed(u32),
    Branch { taken: u32, not_taken: u32 },
    MemoryOperand { register: u32, memory: u32 },
}

impl CycleCost {
    pub const fn fixed(self) -> Option<u32> {
        match self {
            Self::Fixed(cycles) => Some(cycles),
            Self::Branch { .. } | Self::MemoryOperand { .. } => None,
        }
    }

    pub const fn for_branch(self, taken: bool) -> u32 {
        match self {
            Self::Fixed(cycles) => cycles,
            Self::Branch {
                taken: t,
                not_taken,
            } => {
                if taken {
                    t
                } else {
                    not_taken
                }
            }
            Self::MemoryOperand { .. } => unreachable!(),
        }
    }

    pub const fn for_operand8(self, operand: Operand8) -> u32 {
        match self {
            Self::Fixed(cycles) => cycles,
            Self::MemoryOperand { register, memory } => match operand {
                Operand8::R8(6) | Operand8::R8Value(6) => memory,
                _ => register,
            },
            Self::Branch { .. } => unreachable!(),
        }
    }
}

include!(concat!(env!("OUT_DIR"), "/opcode_tables.rs"));
