/// Approximate DMG APU frame sequencer plus initial audio sample buffering.
///
/// This milestone provides timing/domain scaffolding only: generated samples are
/// currently silence, but they are produced at a deterministic cadence and queued
/// for frontend consumption.
#[derive(Debug, Clone)]
pub struct Apu {
    frame_step: u8,
    t_cycle_counter: u32,
    sample_cycle_accumulator: u32,
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
    pub const T_CYCLES_PER_SAMPLE: u32 = Self::DMG_CLOCK_HZ / Self::OUTPUT_SAMPLE_RATE_HZ;

    #[must_use]
    pub const fn new() -> Self {
        Self {
            frame_step: 0,
            t_cycle_counter: 0,
            sample_cycle_accumulator: 0,
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

        let total_sample_cycles = u64::from(self.sample_cycle_accumulator) + u64::from(t_cycles);
        let cycles_per_sample = u64::from(Self::T_CYCLES_PER_SAMPLE);
        let generated_samples = total_sample_cycles / cycles_per_sample;
        self.sample_cycle_accumulator = (total_sample_cycles % cycles_per_sample) as u32;
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

        let _ = apu.tick(Apu::T_CYCLES_PER_SAMPLE * 4 + (Apu::T_CYCLES_PER_SAMPLE - 1));
        assert_eq!(apu.queued_samples(), 4);

        let samples = apu.drain_samples();
        assert_eq!(samples, vec![0, 0, 0, 0]);
        assert_eq!(apu.queued_samples(), 0);
    }
}
