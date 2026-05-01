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
        self.t_cycle_counter = self.t_cycle_counter.saturating_add(t_cycles);

        let mut advanced_steps = 0;
        while self.t_cycle_counter >= Self::FRAME_SEQUENCER_PERIOD_T_CYCLES {
            self.t_cycle_counter -= Self::FRAME_SEQUENCER_PERIOD_T_CYCLES;
            self.frame_step = (self.frame_step + 1) % Self::FRAME_SEQUENCER_STEPS;
            advanced_steps += 1;
        }

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
}
