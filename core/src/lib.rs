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

/// Top-level emulator state container for future subsystem wiring.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Emulator {
    total_cycles: u64,
}

impl Emulator {
    /// Creates a new emulator with initial state.
    pub const fn new() -> Self {
        Self { total_cycles: 0 }
    }

    /// Resets emulator state to defaults.
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// Advances the emulator clock by `cycles` machine cycles.
    ///
    /// This currently updates cycle bookkeeping and will later drive subsystem ticks.
    pub fn step_cycles(&mut self, cycles: u32) {
        self.total_cycles = self.total_cycles.wrapping_add(cycles as u64);
    }

    /// Returns total cycles executed by this emulator instance.
    pub const fn total_cycles(&self) -> u64 {
        self.total_cycles
    }
}
