use std::collections::VecDeque;

use crate::cpu::Registers;

/// Execution event emitted by the emulator while stepping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmulatorEvent {
    CpuStep(CpuStepObservation),
    HaltedFastForward(HaltedFastForwardObservation),
    WatchIo(WatchIoObservation),
}

/// Per-instruction observation with pre/post CPU state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CpuStepObservation {
    pub start_cycle: u64,
    pub end_cycle: u64,
    pub pc_before: u16,
    pub pc_after: u16,
    pub operand1_before: Option<u8>,
    pub operand2_before: Option<u8>,
    pub sp_before: u16,
    pub sp_after: u16,
    pub opcode_hint: Option<u8>,
    pub cycles: u32,
    pub registers_before: Registers,
    pub registers_after: Registers,
    pub ime_before: bool,
    pub ime_after: bool,
    pub halted_before: bool,
    pub halted_after: bool,
    pub interrupt_flag_before: u8,
    pub interrupt_enable_before: u8,
    pub ppu_before: PpuSnapshot,
    pub interrupt_flag: u8,
    pub interrupt_enable: u8,
    pub ppu_after: PpuSnapshot,
    pub unimplemented_opcode: Option<u8>,
}

/// Compact PPU snapshot captured around each CPU step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PpuSnapshot {
    pub lcdc: u8,
    pub stat: u8,
    pub ly: u8,
    pub lyc: u8,
    pub scanline_dot: u16,
    pub lcd_enable_delay_dots: u8,
}

/// Observation emitted when step batching fast-forwards a HALTed CPU.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HaltedFastForwardObservation {
    pub start_cycle: u64,
    pub end_cycle: u64,
    pub pc: u16,
    pub cycles: u64,
    pub interrupt_flag: u8,
    pub interrupt_enable: u8,
}

/// Access type for watchpointed I/O interactions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WatchIoAccessType {
    Read,
    Write,
}

/// Observation emitted for watchpointed MMIO accesses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchIoObservation {
    pub step_start_cycle: u64,
    pub pc: u16,
    pub opcode_hint: Option<u8>,
    pub access_type: WatchIoAccessType,
    pub address: u16,
    pub value: u8,
    pub ppu_mode: u8,
    pub ppu_coincidence: bool,
}

/// Event sink for emulator execution observability.
pub trait EmulatorObserver {
    fn on_event(&mut self, event: EmulatorEvent);

    fn should_stop(&self) -> bool {
        false
    }
}

/// Fixed-size recorder for retaining recent execution events.
#[derive(Debug, Clone)]
pub struct TraceBuffer {
    capacity: usize,
    events: VecDeque<EmulatorEvent>,
}

impl TraceBuffer {
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "TraceBuffer capacity must be positive");
        Self {
            capacity,
            events: VecDeque::with_capacity(capacity),
        }
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &EmulatorEvent> {
        self.events.iter()
    }
}

impl EmulatorObserver for TraceBuffer {
    fn on_event(&mut self, event: EmulatorEvent) {
        if self.events.len() == self.capacity {
            self.events.pop_front();
        }
        self.events.push_back(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trace_buffer_retains_only_recent_events() {
        let mut trace = TraceBuffer::new(2);
        trace.on_event(EmulatorEvent::HaltedFastForward(
            HaltedFastForwardObservation {
                start_cycle: 0,
                end_cycle: 4,
                pc: 0x0001,
                cycles: 4,
                interrupt_flag: 0,
                interrupt_enable: 0,
            },
        ));
        trace.on_event(EmulatorEvent::HaltedFastForward(
            HaltedFastForwardObservation {
                start_cycle: 4,
                end_cycle: 8,
                pc: 0x0001,
                cycles: 4,
                interrupt_flag: 0,
                interrupt_enable: 0,
            },
        ));
        trace.on_event(EmulatorEvent::HaltedFastForward(
            HaltedFastForwardObservation {
                start_cycle: 8,
                end_cycle: 12,
                pc: 0x0001,
                cycles: 4,
                interrupt_flag: 0,
                interrupt_enable: 0,
            },
        ));

        assert_eq!(trace.len(), 2);
        let mut cycles = trace.iter().map(|event| match event {
            EmulatorEvent::HaltedFastForward(observation) => observation.end_cycle,
            EmulatorEvent::CpuStep(_) => 0,
            EmulatorEvent::WatchIo(_) => 0,
        });
        assert_eq!(cycles.next(), Some(8));
        assert_eq!(cycles.next(), Some(12));
    }
}
