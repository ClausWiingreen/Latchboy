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
#[derive(Debug, Default)]
pub struct Emulator;
