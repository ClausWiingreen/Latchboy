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
pub use ppu::{FRAMEBUFFER_HEIGHT, FRAMEBUFFER_LEN, FRAMEBUFFER_WIDTH};

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
    startup_mode: StartupMode,
    total_cycles: u64,
    cycle_carry: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum StartupMode {
    DmgNoBoot,
    BootRom,
}

impl Hash for Emulator {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.cpu.hash(state);
        self.bus.hash(state);
        self.startup_mode.hash(state);
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
    fn next_tick_chunk_size(cycles: u64) -> u32 {
        cycles.min(u64::from(u32::MAX)) as u32
    }

    fn tick_bus_cycles(&mut self, mut cycles: u64) {
        while cycles != 0 {
            let chunk = Self::next_tick_chunk_size(cycles);
            self.bus.tick(chunk);
            cycles -= u64::from(chunk);
        }
    }

    /// Creates a new emulator with a minimal ROM-only cartridge.
    pub fn new() -> Self {
        Self::from_cartridge(default_rom_only_cartridge())
    }

    /// Creates a new emulator from a cartridge image.
    ///
    /// Startup assumptions for this path:
    /// - The DMG boot ROM is skipped.
    /// - CPU registers are initialized to post-boot DMG defaults (`PC=0x0100`, `SP=0xFFFE`).
    /// - Selected I/O registers are initialized with DMG post-boot values via
    ///   [`Bus::apply_dmg_no_boot_defaults`], including `FF50=0x01` (boot ROM unmapped).
    pub fn from_cartridge(cartridge: Cartridge) -> Self {
        let mut bus = Bus::new(cartridge);
        bus.apply_dmg_no_boot_defaults();

        Self {
            cpu: Cpu::new_dmg_no_boot(),
            bus,
            startup_mode: StartupMode::DmgNoBoot,
            total_cycles: 0,
            cycle_carry: 0,
        }
    }

    /// Creates a new emulator from a cartridge image with an explicit DMG boot ROM.
    ///
    /// Startup assumptions for this path:
    /// - Execution begins at `PC=0x0000` with boot ROM mapping enabled.
    /// - The boot ROM is expected to disable itself by writing a non-zero value to `FF50`.
    /// - Once disabled, cartridge ROM is visible at `0x0000..=0x00FF` and cannot be remapped
    ///   until a full emulator reset.
    pub fn from_cartridge_with_boot_rom(cartridge: Cartridge, boot_rom: Vec<u8>) -> Self {
        Self {
            cpu: Cpu::new(),
            bus: Bus::with_boot_rom(cartridge, boot_rom),
            startup_mode: StartupMode::BootRom,
            total_cycles: 0,
            cycle_carry: 0,
        }
    }

    /// Resets CPU and bus state while preserving the loaded cartridge.
    pub fn reset(&mut self) {
        self.bus.reset();
        self.cpu = match self.startup_mode {
            StartupMode::DmgNoBoot => {
                self.bus.apply_dmg_no_boot_defaults();
                Cpu::new_dmg_no_boot()
            }
            StartupMode::BootRom => Cpu::new(),
        };
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

                if (pending_interrupts == 0 || !self.cpu.halted_is_interrupt_wakeable())
                    && !self.bus.timer_may_generate_interrupt()
                    && !self.bus.ppu_may_generate_interrupt()
                {
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
                            cycles: halted_advance,
                            interrupt_flag: self.bus.read8(interrupt_regs::FLAG_REGISTER),
                            interrupt_enable: self.bus.read8(interrupt_regs::ENABLE_REGISTER),
                        },
                    ));
                    self.tick_bus_cycles(halted_advance);
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
            let default_operand1_before = self.bus.read8(pc_before.wrapping_add(1));
            let default_operand2_before = self.bus.read8(pc_before.wrapping_add(2));
            let interrupt_flag = self.bus.read8(interrupt_regs::FLAG_REGISTER);
            let interrupt_enable = self.bus.read8(interrupt_regs::ENABLE_REGISTER);
            let will_service_interrupt = self.cpu.will_service_interrupt(&self.bus);
            let opcode_hint = if halted_before || will_service_interrupt {
                None
            } else {
                Some(self.bus.read8(pc_before))
            };

            let cycles_taken = self.cpu.step(&mut self.bus);
            let operand1_before = self
                .cpu
                .last_step_operand1_fetch()
                .unwrap_or(default_operand1_before);
            let operand2_before = self
                .cpu
                .last_step_operand2_fetch()
                .unwrap_or(default_operand2_before);
            self.tick_bus_cycles(u64::from(cycles_taken));
            available += cycles_taken as u64;

            observer.on_event(EmulatorEvent::CpuStep(CpuStepObservation {
                start_cycle,
                end_cycle: start_cycle.wrapping_add(cycles_taken as u64),
                pc_before,
                pc_after: self.cpu.pc(),
                operand1_before,
                operand2_before,
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

    /// Returns the loaded cartridge.
    ///
    /// Frontends can read battery-backed save RAM via [`Cartridge::save_data`] to persist
    /// post-emulation cartridge state.
    pub const fn cartridge(&self) -> &Cartridge {
        self.bus.cartridge()
    }

    /// Returns mutable access to the loaded cartridge.
    pub fn cartridge_mut(&mut self) -> &mut Cartridge {
        self.bus.cartridge_mut()
    }

    /// Returns `true` once per rendered frame after the PPU enters VBlank.
    ///
    /// Frontends can poll this to know when a complete frame is ready for presentation.
    pub fn take_frame_ready(&mut self) -> bool {
        self.bus.take_frame_ready()
    }

    /// Returns the PPU-owned framebuffer for the most recently rendered frame data.
    ///
    /// Contract:
    /// - Pixel layout is row-major (`index = y * 160 + x`).
    /// - Pixel format is DMG shade indices `0..=3` per byte.
    /// - The slice is borrowed from emulator-owned storage; copy it if the frontend needs
    ///   to retain image data across future mutable emulator calls.
    /// - [`Self::take_frame_ready`] pulses once at each completed frame boundary (VBlank
    ///   entry) and indicates this buffer now contains a coherent full frame.
    pub fn framebuffer_pixels(&self) -> &[u8] {
        self.bus.framebuffer_pixels()
    }

    /// Returns total cycles executed by this emulator instance.
    pub const fn total_cycles(&self) -> u64 {
        self.total_cycles
    }
}

fn default_rom_only_cartridge() -> Cartridge {
    let mut rom = vec![0u8; 2 * 16 * 1024];
    rom[0x0100] = 0x76; // HALT
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
    fn no_boot_startup_uses_dmg_post_boot_defaults() {
        let emulator = Emulator::new();

        let registers = emulator.cpu().registers();
        assert_eq!(registers.a, 0x01);
        assert_eq!(registers.f, 0xB0);
        assert_eq!(registers.b, 0x00);
        assert_eq!(registers.c, 0x13);
        assert_eq!(registers.d, 0x00);
        assert_eq!(registers.e, 0xD8);
        assert_eq!(registers.h, 0x01);
        assert_eq!(registers.l, 0x4D);
        assert_eq!(emulator.cpu().pc(), 0x0100);
        assert_eq!(emulator.cpu().sp(), 0xFFFE);

        assert_eq!(emulator.bus().read8(0xFF40), 0x91);
        assert_eq!(emulator.bus().read8(0xFF47), 0xFC);
        assert_eq!(emulator.bus().read8(0xFF50), 0x01);
        assert_eq!(emulator.bus().read8(0xFFFF), 0x00);
    }

    #[test]
    fn rom_boot_smoke_executes_instruction_stream() {
        let mut rom = vec![0u8; 2 * 16 * 1024];
        rom[0x0100] = 0x31; // LD SP, d16
        rom[0x0101] = 0x00;
        rom[0x0102] = 0xC0;
        rom[0x0103] = 0x3E; // LD A, d8
        rom[0x0104] = 0x42;
        rom[0x0105] = 0xEA; // LD (a16), A
        rom[0x0106] = 0x00;
        rom[0x0107] = 0xC0;
        rom[0x0108] = 0x3C; // INC A
        rom[0x0109] = 0xEA; // LD (a16), A
        rom[0x010A] = 0x01;
        rom[0x010B] = 0xC0;
        rom[0x010C] = 0x76; // HALT

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
        assert_eq!(emulator.cpu().pc(), 0x010D);
        assert_eq!(emulator.cpu().sp(), 0xC000);
        assert_eq!(emulator.cpu().registers().a, 0x43);
        assert_eq!(emulator.bus().read8(0xC000), 0x42);
        assert_eq!(emulator.bus().read8(0xC001), 0x43);
    }

    #[test]
    fn step_cycles_sets_single_frame_ready_signal_on_vblank_transition() {
        let mut emulator = Emulator::new();
        emulator.bus.write8(crate::ppu::LCDC_REGISTER, 0x80);

        emulator.step_cycles(456 * 144);

        assert!(emulator.take_frame_ready());
        assert!(!emulator.take_frame_ready());
    }

    #[test]
    fn boot_rom_startup_path_executes_from_zero_until_ff50_unmaps_boot_rom() {
        let mut rom = vec![0u8; 2 * 16 * 1024];
        rom[0x0100] = 0xD3; // Invalid opcode trap after boot ROM jumps into cartridge space.
        rom[0x0134..0x0138].copy_from_slice(b"BOOT");
        rom[0x0147] = CartridgeType::RomOnly.code();
        rom[0x0148] = RomSize::Banks2.code();
        rom[0x0149] = RamSize::None.code();
        rom[0x014A] = DestinationCode::Japanese.code();
        rom[0x014D] =
            compute_header_checksum(&rom).expect("test rom header checksum should compute");

        let mut boot_rom = vec![0u8; 0x100];
        boot_rom[0x00FC] = 0x3E; // LD A, d8
        boot_rom[0x00FD] = 0x01;
        boot_rom[0x00FE] = 0xE0; // LDH (a8), A
        boot_rom[0x00FF] = 0x50; // -> FF50 (disable boot ROM mapping)

        let cartridge = Cartridge::from_rom(rom).expect("test rom should parse");
        let mut emulator = Emulator::from_cartridge_with_boot_rom(cartridge, boot_rom);

        assert_eq!(emulator.cpu().pc(), 0x0000);
        assert!(emulator.bus().boot_rom_enabled());
        assert_eq!(emulator.bus().read8(0xFF50), 0x00);

        emulator.step_cycles(1008);
        assert_eq!(emulator.cpu().pc(), 0x00FC);
        assert!(emulator.bus().boot_rom_enabled());

        emulator.step_cycles(8);
        assert_eq!(emulator.cpu().pc(), 0x00FE);
        assert_eq!(emulator.cpu().registers().a, 0x01);

        emulator.step_cycles(12);
        assert_eq!(emulator.cpu().pc(), 0x0100);
        assert!(!emulator.bus().boot_rom_enabled());
        assert_eq!(emulator.bus().read8(0xFF50), 0x01);

        emulator.step_cycles(4);
        assert_eq!(emulator.cpu().pc(), 0x0101);
        assert_eq!(emulator.cpu().registers().a, 0x01);
    }

    #[test]
    fn step_cycle_batching_is_deterministic_with_carry() {
        let mut rom = vec![0u8; 2 * 16 * 1024];
        rom[0x0100] = 0x31; // LD SP, d16 (12 cycles)
        rom[0x0101] = 0x34;
        rom[0x0102] = 0x12;
        rom[0x0103] = 0x76; // HALT (4 cycles)

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
    fn halted_fast_forward_keeps_divider_running() {
        let mut emulator = Emulator::new();
        emulator.step_cycles(4);
        assert!(emulator.cpu().halted());
        let div_before = emulator.bus.read8(0xFF04);

        emulator.step_cycles(256);

        assert_eq!(emulator.bus.read8(0xFF04), div_before.wrapping_add(1));
    }

    #[test]
    fn halted_step_does_not_fast_forward_past_ppu_interrupt_sources() {
        let mut rom = vec![0u8; 2 * 16 * 1024];
        rom[0x0100] = 0x3E; // LD A, d8
        rom[0x0101] = 0x01; // enable VBlank in IE
        rom[0x0102] = 0xEA; // LD (a16), A
        rom[0x0103] = 0xFF;
        rom[0x0104] = 0xFF;
        rom[0x0105] = 0x76; // HALT
        rom[0x0134..0x0138].copy_from_slice(b"VBLK");
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
        assert_eq!(emulator.cpu().pc(), 0x0106);

        emulator.step_cycles((456 * 144) + 64);

        assert!(!emulator.cpu().halted());
        assert_ne!(emulator.cpu().pc(), 0x0106);
        assert_ne!(emulator.bus.read8(0xFF0F) & 0x01, 0x00);
    }

    #[test]
    fn tick_bus_cycles_chunks_values_above_u32_max() {
        let mut remaining = u64::from(u32::MAX) + 1;
        let first = Emulator::next_tick_chunk_size(remaining);
        remaining -= u64::from(first);
        let second = Emulator::next_tick_chunk_size(remaining);

        assert_eq!(first, u32::MAX);
        assert_eq!(second, 1);
    }

    #[test]
    fn halted_cpu_resumes_when_if_and_ie_are_pending() {
        let mut emulator = Emulator::new();
        emulator.step_cycles(4);
        assert!(emulator.cpu().halted());
        assert_eq!(emulator.cpu().pc(), 0x0101);

        emulator.bus.write8(0xFF0F, 0x01);
        emulator.bus.write8(0xFFFF, 0x01);
        emulator.step_cycles(4);

        assert!(!emulator.cpu().halted());
        assert_eq!(emulator.cpu().pc(), 0x0102);
    }

    #[test]
    fn halted_cpu_services_interrupt_when_ime_is_enabled() {
        let mut rom = vec![0u8; 2 * 16 * 1024];
        rom[0x0100] = 0xFB; // EI
        rom[0x0101] = 0x76; // HALT
        rom[0x0102] = 0x00; // NOP (must not execute when interrupt services first)
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
        assert_eq!(emulator.cpu().pc(), 0x0102);
        assert!(emulator.cpu().ime());

        emulator.bus.write8(0xFF0F, 0x01);
        emulator.bus.write8(0xFFFF, 0x01);
        emulator.step_cycles(20);

        assert_eq!(emulator.cpu().pc(), 0x0040);
        assert!(!emulator.cpu().ime());
        assert_eq!(emulator.bus.read8(0xFF0F), 0x00);
        assert_eq!(emulator.bus.read8(0xFFFC), 0x02);
        assert_eq!(emulator.bus.read8(0xFFFD), 0x01);
    }

    #[test]
    fn hash_reflects_bus_memory_state() {
        use std::collections::hash_map::DefaultHasher;

        let mut rom_a = vec![0u8; 2 * 16 * 1024];
        rom_a[0x0100] = 0x76;
        rom_a[0x0134..0x0138].copy_from_slice(b"HSHA");
        rom_a[0x0147] = CartridgeType::RomOnly.code();
        rom_a[0x0148] = RomSize::Banks2.code();
        rom_a[0x0149] = RamSize::None.code();
        rom_a[0x014A] = DestinationCode::Japanese.code();
        rom_a[0x014D] =
            compute_header_checksum(&rom_a).expect("test rom header checksum should compute");

        let mut rom_b = rom_a.clone();
        rom_b[0x0101] = 0xAA;

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
        rom[0x0100] = 0x3E; // LD A, d8
        rom[0x0101] = 0x99;
        rom[0x0102] = 0x76; // HALT

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
        assert_eq!(emulator.cpu().pc(), 0x0103);
    }

    #[test]
    fn reset_restores_startup_state_for_each_startup_mode() {
        let mut rom = vec![0u8; 2 * 16 * 1024];
        rom[0x0100] = 0x00; // NOP
        rom[0x0134..0x0138].copy_from_slice(b"STRT");
        rom[0x0147] = CartridgeType::RomOnly.code();
        rom[0x0148] = RomSize::Banks2.code();
        rom[0x0149] = RamSize::None.code();
        rom[0x014A] = DestinationCode::Japanese.code();
        rom[0x014D] =
            compute_header_checksum(&rom).expect("test rom header checksum should compute");
        let cartridge = Cartridge::from_rom(rom).expect("test rom should parse");

        let mut no_boot = Emulator::from_cartridge(cartridge.clone());
        no_boot.step_cycles(4);
        no_boot.reset();
        assert_eq!(no_boot.cpu().pc(), 0x0100);
        assert_eq!(no_boot.bus().read8(0xFF50), 0x01);
        assert!(!no_boot.bus().boot_rom_enabled());

        let mut boot_rom = vec![0u8; 0x100];
        boot_rom[0x0000] = 0x3E; // LD A, d8
        boot_rom[0x0001] = 0x01;
        boot_rom[0x0002] = 0xE0; // LDH (a8), A
        boot_rom[0x0003] = 0x50; // -> FF50
        let mut with_boot = Emulator::from_cartridge_with_boot_rom(cartridge, boot_rom);
        with_boot.step_cycles(16);
        assert!(!with_boot.bus().boot_rom_enabled());

        with_boot.reset();
        assert_eq!(with_boot.cpu().pc(), 0x0000);
        assert_eq!(with_boot.bus().read8(0xFF50), 0x00);
        assert!(with_boot.bus().boot_rom_enabled());
    }

    #[test]
    fn step_cycles_observer_captures_opcode_flow() {
        use crate::observability::{EmulatorEvent, TraceBuffer};

        let mut rom = vec![0u8; 2 * 16 * 1024];
        rom[0x0100] = 0x00; // NOP
        rom[0x0101] = 0x00; // NOP
        rom[0x0102] = 0x76; // HALT

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

    #[test]
    fn observer_suppresses_opcode_hint_when_interrupt_is_serviced() {
        use crate::observability::{EmulatorEvent, TraceBuffer};

        let mut rom = vec![0u8; 2 * 16 * 1024];
        rom[0x0100] = 0xFB; // EI
        rom[0x0101] = 0x76; // HALT

        rom[0x0134..0x0138].copy_from_slice(b"INTT");
        rom[0x0147] = CartridgeType::RomOnly.code();
        rom[0x0148] = RomSize::Banks2.code();
        rom[0x0149] = RamSize::None.code();
        rom[0x014A] = DestinationCode::Japanese.code();
        rom[0x014D] =
            compute_header_checksum(&rom).expect("test rom header checksum should compute");

        let cartridge = Cartridge::from_rom(rom).expect("test rom should parse");
        let mut emulator = Emulator::from_cartridge(cartridge);

        emulator.step_cycles(8);
        emulator.bus.write8(0xFF0F, 0x01);
        emulator.bus.write8(0xFFFF, 0x01);

        let mut trace = TraceBuffer::new(8);
        emulator.step_cycles_with_observer(20, &mut trace);

        let interrupt_event = trace.iter().find_map(|event| match event {
            EmulatorEvent::CpuStep(observation) if observation.pc_after == 0x0040 => {
                Some(observation)
            }
            _ => None,
        });

        let interrupt_event = interrupt_event.expect("interrupt service step should be traced");
        assert_eq!(interrupt_event.opcode_hint, None);
    }
}
