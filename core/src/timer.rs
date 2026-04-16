use crate::interrupts;

pub const DIV_REGISTER: u16 = 0xFF04;
pub const TIMA_REGISTER: u16 = 0xFF05;
pub const TMA_REGISTER: u16 = 0xFF06;
pub const TAC_REGISTER: u16 = 0xFF07;

const TAC_ENABLE_MASK: u8 = 0b0000_0100;
const TAC_CLOCK_SELECT_MASK: u8 = 0b0000_0011;
const TAC_UNUSED_MASK: u8 = 0b1111_1000;
const TIMER_INTERRUPT_MASK: u8 = 0b0000_0100;
const TIMER_RELOAD_DELAY_CYCLES: u8 = 4;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct Timer {
    divider: u16,
    tima: u8,
    tma: u8,
    tac: u8,
    overflow_reload_delay: Option<u8>,
}

impl Timer {
    pub fn read(&self, address: u16) -> u8 {
        match address {
            DIV_REGISTER => self.divider.to_be_bytes()[0],
            TIMA_REGISTER => self.tima,
            TMA_REGISTER => self.tma,
            TAC_REGISTER => self.tac | TAC_UNUSED_MASK,
            _ => unreachable!("invalid timer register read: {address:#06X}"),
        }
    }

    pub fn write(&mut self, address: u16, value: u8) {
        match address {
            DIV_REGISTER => {
                let previous_input = self.timer_input();
                self.divider = 0;
                self.apply_falling_edge_if_needed(previous_input, self.timer_input());
            }
            TIMA_REGISTER => {
                self.tima = value;
                self.overflow_reload_delay = None;
            }
            TMA_REGISTER => self.tma = value,
            TAC_REGISTER => {
                let previous_input = self.timer_input();
                self.tac = value & (TAC_ENABLE_MASK | TAC_CLOCK_SELECT_MASK);
                self.apply_falling_edge_if_needed(previous_input, self.timer_input());
            }
            _ => unreachable!("invalid timer register write: {address:#06X}"),
        }
    }

    pub fn step(&mut self, interrupt_flag: &mut u8) {
        self.advance_reload_state(interrupt_flag);

        let previous_input = self.timer_input();
        self.divider = self.divider.wrapping_add(1);
        self.apply_falling_edge_if_needed(previous_input, self.timer_input());
    }

    pub const fn timer_may_generate_interrupt(&self) -> bool {
        (self.tac & TAC_ENABLE_MASK) != 0 || self.overflow_reload_delay.is_some()
    }

    fn selected_divider_bit(&self) -> u16 {
        match self.tac & TAC_CLOCK_SELECT_MASK {
            0b00 => 9,
            0b01 => 3,
            0b10 => 5,
            0b11 => 7,
            _ => unreachable!(),
        }
    }

    fn timer_input(&self) -> bool {
        let timer_enabled = (self.tac & TAC_ENABLE_MASK) != 0;
        timer_enabled && ((self.divider >> self.selected_divider_bit()) & 0x1) != 0
    }

    fn apply_falling_edge_if_needed(&mut self, previous_input: bool, current_input: bool) {
        if previous_input && !current_input {
            self.increment_tima();
        }
    }

    fn increment_tima(&mut self) {
        if self.overflow_reload_delay.is_some() {
            return;
        }

        let (next_tima, overflowed) = self.tima.overflowing_add(1);
        self.tima = next_tima;

        if overflowed {
            self.tima = 0x00;
            self.overflow_reload_delay = Some(TIMER_RELOAD_DELAY_CYCLES);
        }
    }

    fn advance_reload_state(&mut self, interrupt_flag: &mut u8) {
        let Some(mut cycles_remaining) = self.overflow_reload_delay else {
            return;
        };

        cycles_remaining -= 1;
        if cycles_remaining == 0 {
            self.tima = self.tma;
            *interrupt_flag |= TIMER_INTERRUPT_MASK & interrupts::MASK;
            self.overflow_reload_delay = None;
        } else {
            self.overflow_reload_delay = Some(cycles_remaining);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timer_increments_on_falling_edges_of_selected_divider_bit() {
        let mut timer = Timer::default();
        timer.write(TAC_REGISTER, 0b101);

        let mut interrupt_flag = 0;
        for _ in 0..63 {
            timer.step(&mut interrupt_flag);
        }

        assert_eq!(timer.read(TIMA_REGISTER), 3);
        assert_eq!(interrupt_flag & TIMER_INTERRUPT_MASK, 0);
    }

    #[test]
    fn divider_reset_can_trigger_falling_edge_increment() {
        let mut timer = Timer::default();
        let mut interrupt_flag = 0;
        timer.write(TAC_REGISTER, 0b101);

        for _ in 0..8 {
            timer.step(&mut interrupt_flag);
        }

        assert_eq!(timer.read(TIMA_REGISTER), 0);
        timer.write(DIV_REGISTER, 0x00);
        assert_eq!(timer.read(TIMA_REGISTER), 1);
    }

    #[test]
    fn overflow_reloads_tma_and_requests_interrupt_after_delay() {
        let mut timer = Timer::default();
        let mut interrupt_flag = 0;
        timer.write(TAC_REGISTER, 0b101);
        timer.write(TMA_REGISTER, 0xAB);
        timer.write(TIMA_REGISTER, 0xFF);

        for _ in 0..16 {
            timer.step(&mut interrupt_flag);
        }

        assert_eq!(timer.read(TIMA_REGISTER), 0x00);
        assert_eq!(interrupt_flag & TIMER_INTERRUPT_MASK, 0);

        for _ in 0..4 {
            timer.step(&mut interrupt_flag);
        }

        assert_eq!(timer.read(TIMA_REGISTER), 0xAB);
        assert_ne!(interrupt_flag & TIMER_INTERRUPT_MASK, 0);
    }

    #[test]
    fn writing_tima_during_reload_delay_cancels_pending_reload() {
        let mut timer = Timer::default();
        let mut interrupt_flag = 0;
        timer.write(TAC_REGISTER, 0b101);
        timer.write(TMA_REGISTER, 0xAB);
        timer.write(TIMA_REGISTER, 0xFF);

        for _ in 0..8 {
            timer.step(&mut interrupt_flag);
        }

        timer.write(TIMA_REGISTER, 0x77);

        for _ in 0..4 {
            timer.step(&mut interrupt_flag);
        }

        assert_eq!(timer.read(TIMA_REGISTER), 0x77);
        assert_eq!(interrupt_flag & TIMER_INTERRUPT_MASK, 0);
    }

    #[test]
    fn pending_reload_is_interrupt_capable_even_if_tac_is_disabled() {
        let mut timer = Timer::default();
        let mut interrupt_flag = 0;
        timer.write(TAC_REGISTER, 0b101);
        timer.write(TIMA_REGISTER, 0xFF);

        for _ in 0..16 {
            timer.step(&mut interrupt_flag);
        }

        timer.write(TAC_REGISTER, 0x00);
        assert!(timer.timer_may_generate_interrupt());

        for _ in 0..4 {
            timer.step(&mut interrupt_flag);
        }

        assert_ne!(interrupt_flag & TIMER_INTERRUPT_MASK, 0);
        assert!(!timer.timer_may_generate_interrupt());
    }
}
