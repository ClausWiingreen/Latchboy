/// Approximate DMG APU frame sequencer plus initial audio sample buffering.
///
/// This milestone provides timing/domain scaffolding only: generated samples are
/// currently silence, but they are produced at a deterministic cadence and queued
/// for frontend consumption.
#[derive(Debug, Clone)]
pub struct Apu {
    frame_step: u8,
    t_cycle_counter: u32,
    sample_phase: u64,
    sample_buffer: Vec<i16>,
}

impl Default for Apu {
    fn default() -> Self {
        Self::new()
    }
}

impl Apu {
    pub const FRAME_SEQUENCER_PERIOD_T_CYCLES: u32 = 8_192;
    pub const FRAME_SEQUENCER_STEPS: u8 = 8;
    pub const DMG_CLOCK_HZ: u32 = 4_194_304;
    pub const OUTPUT_SAMPLE_RATE_HZ: u32 = 48_000;

    #[must_use]
    pub const fn new() -> Self {
        Self {
            frame_step: 0,
            t_cycle_counter: 0,
            sample_phase: 0,
            sample_buffer: Vec::new(),
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

        self.sample_phase += u64::from(t_cycles) * u64::from(Self::OUTPUT_SAMPLE_RATE_HZ);
        let generated_samples = self.sample_phase / u64::from(Self::DMG_CLOCK_HZ);
        self.sample_phase %= u64::from(Self::DMG_CLOCK_HZ);
        for _ in 0..generated_samples {
            self.sample_buffer.push(0);
        }

        advanced_steps
    }

    #[must_use]
    pub const fn frame_step(&self) -> u8 {
        self.frame_step
    }

    #[must_use]
    pub fn queued_samples(&self) -> usize {
        self.sample_buffer.len()
    }

    pub fn drain_samples(&mut self) -> Vec<i16> {
        self.sample_buffer.drain(..).collect()
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

    #[test]
    fn tick_queues_silence_samples_at_fixed_cadence() {
        let mut apu = Apu::new();

        // 4 exact sample periods worth of t-cycles, plus one cycle short of a fifth.
        let cycles_per_four_samples =
            (u64::from(Apu::DMG_CLOCK_HZ) * 4) / u64::from(Apu::OUTPUT_SAMPLE_RATE_HZ);
        let _ = apu.tick((cycles_per_four_samples + 86) as u32);
        assert_eq!(apu.queued_samples(), 4);

        let samples = apu.drain_samples();
        assert_eq!(samples, vec![0, 0, 0, 0]);
        assert_eq!(apu.queued_samples(), 0);
    }

    #[test]
    fn sample_generation_preserves_fractional_remainder_across_ticks() {
        let mut apu = Apu::new();

        let _ = apu.tick(87);
        assert_eq!(apu.queued_samples(), 0);
        let _ = apu.tick(1);
        assert_eq!(apu.queued_samples(), 1);
    }
}
