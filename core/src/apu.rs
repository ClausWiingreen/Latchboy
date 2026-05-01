/// Approximate DMG APU frame sequencer.
///
/// The frame sequencer advances at 512 Hz (one step every 8192 T-cycles)
/// and loops over 8 steps.
#[derive(Debug, Clone)]
pub struct Apu {
    frame_step: u8,
    t_cycle_counter: u32,
}

impl Default for Apu {
    fn default() -> Self {
        Self::new()
    }
}

impl Apu {
    pub const FRAME_SEQUENCER_PERIOD_T_CYCLES: u32 = 8_192;
    pub const FRAME_SEQUENCER_STEPS: u8 = 8;

    #[must_use]
    pub const fn new() -> Self {
        Self {
            frame_step: 0,
            t_cycle_counter: 0,
        }
    }

    /// Advances the APU timing domain by `t_cycles` machine t-cycles.
    ///
    /// Returns the number of frame-sequencer steps advanced.
    #[must_use]
    pub fn tick(&mut self, t_cycles: u32) -> u32 {
        let total_t_cycles = u64::from(self.t_cycle_counter) + u64::from(t_cycles);
        let period = u64::from(Self::FRAME_SEQUENCER_PERIOD_T_CYCLES);

        let advanced_steps = (total_t_cycles / period) as u32;
        self.t_cycle_counter = (total_t_cycles % period) as u32;
        self.frame_step = ((u32::from(self.frame_step) + advanced_steps)
            % u32::from(Self::FRAME_SEQUENCER_STEPS)) as u8;

        advanced_steps
    }

    #[must_use]
    pub const fn frame_step(&self) -> u8 {
        self.frame_step
    }
}

#[cfg(test)]
mod tests {
    use super::Apu;

    #[test]
    fn frame_sequencer_does_not_advance_before_threshold() {
        let mut apu = Apu::new();

        let advanced = apu.tick(Apu::FRAME_SEQUENCER_PERIOD_T_CYCLES - 1);

        assert_eq!(advanced, 0);
        assert_eq!(apu.frame_step(), 0);
    }

    #[test]
    fn frame_sequencer_advances_once_at_threshold() {
        let mut apu = Apu::new();

        let advanced = apu.tick(Apu::FRAME_SEQUENCER_PERIOD_T_CYCLES);

        assert_eq!(advanced, 1);
        assert_eq!(apu.frame_step(), 1);
    }

    #[test]
    fn frame_sequencer_wraps_after_eight_steps() {
        let mut apu = Apu::new();

        let advanced =
            apu.tick(Apu::FRAME_SEQUENCER_PERIOD_T_CYCLES * u32::from(Apu::FRAME_SEQUENCER_STEPS));

        assert_eq!(advanced, u32::from(Apu::FRAME_SEQUENCER_STEPS));
        assert_eq!(apu.frame_step(), 0);
    }

    #[test]
    fn frame_sequencer_handles_large_tick_without_losing_steps() {
        let mut apu = Apu::new();
        let _ = apu.tick(Apu::FRAME_SEQUENCER_PERIOD_T_CYCLES - 1);

        let advanced = apu.tick(u32::MAX);
        let expected = (u64::from(Apu::FRAME_SEQUENCER_PERIOD_T_CYCLES - 1) + u64::from(u32::MAX))
            / u64::from(Apu::FRAME_SEQUENCER_PERIOD_T_CYCLES);

        assert_eq!(u64::from(advanced), expected);
    }
}
