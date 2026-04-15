pub mod apu;
pub mod bus;
pub mod cartridge;
pub mod cpu;
pub mod frontend;
pub mod input;
pub mod interrupts;
pub mod observability;
pub mod ppu;
pub mod serial;
pub mod timer;

use bus::Bus;
use cartridge::{
    compute_header_checksum, Cartridge, CartridgeType, DestinationCode, RamSize, RomSize,
};
use cpu::Cpu;
use interrupts as interrupt_regs;
use observability::{
    CpuStepObservation, EmulatorEvent, EmulatorObserver, HaltedFastForwardObservation,
};
use std::hash::{Hash, Hasher};

/// Top-level emulator state container for subsystem wiring.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Emulator {
    cpu: Cpu,
    bus: Bus,
    total_cycles: u64,
    cycle_carry: u32,
}

impl Hash for Emulator {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.cpu.hash(state);
        self.bus.hash(state);
        self.total_cycles.hash(state);
        self.cycle_carry.hash(state);
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
            cycle_carry: 0,
        }
    }

    /// Resets CPU and bus state while preserving the loaded cartridge.
    pub fn reset(&mut self) {
        self.cpu = Cpu::new();
        self.bus.reset();
        self.total_cycles = 0;
        self.cycle_carry = 0;
    }

    /// Advances execution by at least `cycles` machine cycles.
    pub fn step_cycles(&mut self, cycles: u32) {
        struct NoopObserver;
        impl EmulatorObserver for NoopObserver {
            fn on_event(&mut self, _event: EmulatorEvent) {}
        }

        let mut observer = NoopObserver;
        self.step_cycles_with_observer(cycles, &mut observer);
    }

    /// Advances execution by at least `cycles` machine cycles and emits detailed execution events.
    pub fn step_cycles_with_observer<O: EmulatorObserver>(
        &mut self,
        cycles: u32,
        observer: &mut O,
    ) {
        let target = cycles as u64;
        let mut available = self.cycle_carry as u64;

        while available < target {
            if self.cpu.halted() {
                let pending_interrupts = self.bus.read8(interrupt_regs::FLAG_REGISTER)
                    & self.bus.read8(interrupt_regs::ENABLE_REGISTER)
                    & interrupt_regs::MASK;

                if pending_interrupts == 0 || !self.cpu.halted_is_interrupt_wakeable() {
                    let remaining = target - available;
                    let halted_advance = remaining.div_ceil(4) * 4;
                    observer.on_event(EmulatorEvent::HaltedFastForward(
                        HaltedFastForwardObservation {
                            start_cycle: self.total_cycles.wrapping_add(available),
                            end_cycle: self
                                .total_cycles
                                .wrapping_add(available)
                                .wrapping_add(halted_advance),
                            pc: self.cpu.pc(),
                            cycles: halted_advance as u32,
                            interrupt_flag: self.bus.read8(interrupt_regs::FLAG_REGISTER),
                            interrupt_enable: self.bus.read8(interrupt_regs::ENABLE_REGISTER),
                        },
                    ));
                    available += halted_advance;
                    break;
                }
            }

            let start_cycle = self.total_cycles.wrapping_add(available);
            let pc_before = self.cpu.pc();
            let sp_before = self.cpu.sp();
            let registers_before = *self.cpu.registers();
            let ime_before = self.cpu.ime();
            let halted_before = self.cpu.halted();
            let interrupt_flag = self.bus.read8(interrupt_regs::FLAG_REGISTER);
            let interrupt_enable = self.bus.read8(interrupt_regs::ENABLE_REGISTER);
            let opcode_hint = (!halted_before).then_some(self.bus.read8(pc_before));

            let cycles_taken = self.cpu.step(&mut self.bus);
            available += cycles_taken as u64;

            observer.on_event(EmulatorEvent::CpuStep(CpuStepObservation {
                start_cycle,
                end_cycle: start_cycle.wrapping_add(cycles_taken as u64),
                pc_before,
                pc_after: self.cpu.pc(),
                sp_before,
                sp_after: self.cpu.sp(),
                opcode_hint,
                cycles: cycles_taken,
                registers_before,
                registers_after: *self.cpu.registers(),
                ime_before,
                ime_after: self.cpu.ime(),
                halted_before,
                halted_after: self.cpu.halted(),
                interrupt_flag,
                interrupt_enable,
            }));
        }

        self.cycle_carry = (available - target) as u32;
        self.total_cycles = self.total_cycles.wrapping_add(target);
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

    #[test]
    fn step_cycle_batching_is_deterministic_with_carry() {
        let mut rom = vec![0u8; 2 * 16 * 1024];
        rom[0x0000] = 0x31; // LD SP, d16 (12 cycles)
        rom[0x0001] = 0x34;
        rom[0x0002] = 0x12;
        rom[0x0003] = 0x76; // HALT (4 cycles)

        rom[0x0134..0x0138].copy_from_slice(b"CARR");
        rom[0x0147] = CartridgeType::RomOnly.code();
        rom[0x0148] = RomSize::Banks2.code();
        rom[0x0149] = RamSize::None.code();
        rom[0x014A] = DestinationCode::Japanese.code();
        rom[0x014D] =
            compute_header_checksum(&rom).expect("test rom header checksum should compute");

        let cartridge = Cartridge::from_rom(rom).expect("test rom should parse");

        let mut split = Emulator::from_cartridge(cartridge.clone());
        split.step_cycles(8);
        split.step_cycles(4);

        let mut single = Emulator::from_cartridge(cartridge);
        single.step_cycles(12);

        assert_eq!(split.cpu().pc(), single.cpu().pc());
        assert_eq!(split.cpu().sp(), single.cpu().sp());
        assert_eq!(split.cpu().halted(), single.cpu().halted());
        assert_eq!(split.total_cycles(), single.total_cycles());
    }

    #[test]
    fn large_step_on_halted_cpu_fast_forwards_without_state_changes() {
        let mut emulator = Emulator::new();
        assert!(!emulator.cpu().halted());

        emulator.step_cycles(4);
        assert!(emulator.cpu().halted());
        let halted_pc = emulator.cpu().pc();

        emulator.step_cycles(1_000_000);

        assert!(emulator.cpu().halted());
        assert_eq!(emulator.cpu().pc(), halted_pc);
        assert_eq!(emulator.total_cycles(), 1_000_004);
    }

    #[test]
    fn halted_cpu_resumes_when_if_and_ie_are_pending() {
        let mut emulator = Emulator::new();
        emulator.step_cycles(4);
        assert!(emulator.cpu().halted());
        assert_eq!(emulator.cpu().pc(), 0x0001);

        emulator.bus.write8(0xFF0F, 0x01);
        emulator.bus.write8(0xFFFF, 0x01);
        emulator.step_cycles(4);

        assert!(!emulator.cpu().halted());
        assert_eq!(emulator.cpu().pc(), 0x0002);
    }

    #[test]
    fn halted_cpu_services_interrupt_when_ime_is_enabled() {
        let mut rom = vec![0u8; 2 * 16 * 1024];
        rom[0x0000] = 0xFB; // EI
        rom[0x0001] = 0x76; // HALT
        rom[0x0002] = 0x00; // NOP (must not execute when interrupt services first)
        rom[0x0134..0x0138].copy_from_slice(b"INTS");
        rom[0x0147] = CartridgeType::RomOnly.code();
        rom[0x0148] = RomSize::Banks2.code();
        rom[0x0149] = RamSize::None.code();
        rom[0x014A] = DestinationCode::Japanese.code();
        rom[0x014D] =
            compute_header_checksum(&rom).expect("test rom header checksum should compute");

        let cartridge = Cartridge::from_rom(rom).expect("test rom should parse");
        let mut emulator = Emulator::from_cartridge(cartridge);

        emulator.step_cycles(8);
        assert!(emulator.cpu().halted());
        assert_eq!(emulator.cpu().pc(), 0x0002);
        assert!(emulator.cpu().ime());

        emulator.bus.write8(0xFF0F, 0x01);
        emulator.bus.write8(0xFFFF, 0x01);
        emulator.step_cycles(20);

        assert_eq!(emulator.cpu().pc(), 0x0040);
        assert!(!emulator.cpu().ime());
        assert_eq!(emulator.bus.read8(0xFF0F), 0x00);
        assert_eq!(emulator.bus.read8(0xFFFC), 0x02);
        assert_eq!(emulator.bus.read8(0xFFFD), 0x00);
    }

    #[test]
    fn hash_reflects_bus_memory_state() {
        use std::collections::hash_map::DefaultHasher;

        let mut rom_a = vec![0u8; 2 * 16 * 1024];
        rom_a[0x0000] = 0x76;
        rom_a[0x0134..0x0138].copy_from_slice(b"HSHA");
        rom_a[0x0147] = CartridgeType::RomOnly.code();
        rom_a[0x0148] = RomSize::Banks2.code();
        rom_a[0x0149] = RamSize::None.code();
        rom_a[0x014A] = DestinationCode::Japanese.code();
        rom_a[0x014D] =
            compute_header_checksum(&rom_a).expect("test rom header checksum should compute");

        let mut rom_b = rom_a.clone();
        rom_b[0x0001] = 0xAA;

        let emu_a =
            Emulator::from_cartridge(Cartridge::from_rom(rom_a).expect("test rom should parse"));
        let emu_b =
            Emulator::from_cartridge(Cartridge::from_rom(rom_b).expect("test rom should parse"));

        let mut hasher_a = DefaultHasher::new();
        emu_a.hash(&mut hasher_a);

        let mut hasher_b = DefaultHasher::new();
        emu_b.hash(&mut hasher_b);

        assert_ne!(hasher_a.finish(), hasher_b.finish());
    }

    #[test]
    fn reset_preserves_loaded_cartridge_program() {
        let mut rom = vec![0u8; 2 * 16 * 1024];
        rom[0x0000] = 0x3E; // LD A, d8
        rom[0x0001] = 0x99;
        rom[0x0002] = 0x76; // HALT

        rom[0x0134..0x0138].copy_from_slice(b"RSET");
        rom[0x0147] = CartridgeType::RomOnly.code();
        rom[0x0148] = RomSize::Banks2.code();
        rom[0x0149] = RamSize::None.code();
        rom[0x014A] = DestinationCode::Japanese.code();
        rom[0x014D] =
            compute_header_checksum(&rom).expect("test rom header checksum should compute");

        let cartridge = Cartridge::from_rom(rom).expect("test rom should parse");
        let mut emulator = Emulator::from_cartridge(cartridge);

        emulator.step_cycles(16);
        assert_eq!(emulator.cpu().registers().a, 0x99);

        emulator.reset();
        emulator.step_cycles(16);

        assert_eq!(emulator.cpu().registers().a, 0x99);
        assert_eq!(emulator.cpu().pc(), 0x0003);
    }

    #[test]
    fn step_cycles_observer_captures_opcode_flow() {
        use crate::observability::{EmulatorEvent, TraceBuffer};

        let mut rom = vec![0u8; 2 * 16 * 1024];
        rom[0x0000] = 0x00; // NOP
        rom[0x0001] = 0x00; // NOP
        rom[0x0002] = 0x76; // HALT

        rom[0x0134..0x0138].copy_from_slice(b"OBSV");
        rom[0x0147] = CartridgeType::RomOnly.code();
        rom[0x0148] = RomSize::Banks2.code();
        rom[0x0149] = RamSize::None.code();
        rom[0x014A] = DestinationCode::Japanese.code();
        rom[0x014D] =
            compute_header_checksum(&rom).expect("test rom header checksum should compute");

        let cartridge = Cartridge::from_rom(rom).expect("test rom should parse");
        let mut emulator = Emulator::from_cartridge(cartridge);
        let mut trace = TraceBuffer::new(8);

        emulator.step_cycles_with_observer(12, &mut trace);

        let opcodes: Vec<Option<u8>> = trace
            .iter()
            .filter_map(|event| match event {
                EmulatorEvent::CpuStep(observation) => Some(observation.opcode_hint),
                EmulatorEvent::HaltedFastForward(_) => None,
            })
            .collect();
        assert_eq!(opcodes, vec![Some(0x00), Some(0x00), Some(0x76)]);
    }
}
