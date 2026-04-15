use std::collections::VecDeque;

use crate::cpu::Registers;

/// Execution event emitted by the emulator while stepping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmulatorEvent {
    CpuStep(CpuStepObservation),
    HaltedFastForward(HaltedFastForwardObservation),
}

/// Per-instruction observation with pre/post CPU state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CpuStepObservation {
    pub start_cycle: u64,
    pub end_cycle: u64,
    pub pc_before: u16,
    pub pc_after: u16,
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
    pub interrupt_flag: u8,
    pub interrupt_enable: u8,
}

/// Observation emitted when step batching fast-forwards a HALTed CPU.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HaltedFastForwardObservation {
    pub start_cycle: u64,
    pub end_cycle: u64,
    pub pc: u16,
    pub cycles: u32,
    pub interrupt_flag: u8,
    pub interrupt_enable: u8,
}

/// Event sink for emulator execution observability.
pub trait EmulatorObserver {
    fn on_event(&mut self, event: EmulatorEvent);
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
        });
        assert_eq!(cycles.next(), Some(8));
        assert_eq!(cycles.next(), Some(12));
    }
}
