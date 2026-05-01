/// Approximate DMG APU frame sequencer plus initial audio sample buffering.
///
/// This milestone provides deterministic timing plus a configurable Channel 1 square
/// wave path with basic sweep handling (`NR10` + `NR13/NR14`-style frequency updates).
#[derive(Debug, Clone)]
pub struct Apu {
    frame_step: u8,
    t_cycle_counter: u32,
    sample_phase: u64,
    sample_buffer: Vec<i16>,
    ch1_phase_accumulator: u32,
    ch1: Ch1,
}

#[derive(Debug, Clone)]
struct Ch1 {
    frequency_hz: u32,
    duty: DutyCycle,
    amplitude: i16,
    sweep_period_steps: u8,
    sweep_shift: u8,
    sweep_negate: bool,
    sweep_tick_counter: u8,
}

#[derive(Debug, Clone, Copy)]
enum DutyCycle {
    Duty12_5,
    Duty25,
    Duty50,
    Duty75,
}

impl DutyCycle {
    const fn high_numerator(self) -> u8 {
        match self {
            Self::Duty12_5 => 1,
            Self::Duty25 => 2,
            Self::Duty50 => 4,
            Self::Duty75 => 6,
        }
    }

    const fn denominator(self) -> u8 {
        8
    }
}


impl Ch1 {
    const fn effective_sweep_period_steps(&self) -> u8 {
        if self.sweep_period_steps == 0 {
            8
        } else {
            self.sweep_period_steps
        }
    }
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

    const CH1_DEFAULT_FREQUENCY_HZ: u32 = 440;
    const CH1_DEFAULT_AMPLITUDE: i16 = 2_500;

    #[must_use]
    pub const fn new() -> Self {
        Self {
            frame_step: 0,
            t_cycle_counter: 0,
            sample_phase: 0,
            sample_buffer: Vec::new(),
            ch1_phase_accumulator: 0,
            ch1: Ch1 {
                frequency_hz: Self::CH1_DEFAULT_FREQUENCY_HZ,
                duty: DutyCycle::Duty50,
                amplitude: Self::CH1_DEFAULT_AMPLITUDE,
                sweep_period_steps: 0,
                sweep_shift: 0,
                sweep_negate: false,
                sweep_tick_counter: 0,
            },
        }
    }

    #[must_use]
    pub fn tick(&mut self, t_cycles: u32) -> u32 {
        let total_t_cycles = u64::from(self.t_cycle_counter) + u64::from(t_cycles);
        let period = u64::from(Self::FRAME_SEQUENCER_PERIOD_T_CYCLES);

        let advanced_steps = (total_t_cycles / period) as u32;
        self.t_cycle_counter = (total_t_cycles % period) as u32;

        for _ in 0..advanced_steps {
            self.frame_step = (self.frame_step + 1) % Self::FRAME_SEQUENCER_STEPS;
            self.apply_ch1_sweep_on_frame_step();
        }

        self.sample_phase += u64::from(t_cycles) * u64::from(Self::OUTPUT_SAMPLE_RATE_HZ);
        let generated_samples = self.sample_phase / u64::from(Self::DMG_CLOCK_HZ);
        self.sample_phase %= u64::from(Self::DMG_CLOCK_HZ);
        for _ in 0..generated_samples {
            let sample = self.next_ch1_sample();
            self.sample_buffer.push(sample);
        }

        advanced_steps
    }

    fn apply_ch1_sweep_on_frame_step(&mut self) {
        if !matches!(self.frame_step, 2 | 6) {
            return;
        }
        if self.ch1.sweep_shift == 0 {
            return;
        }

        let sweep_period = self.ch1.effective_sweep_period_steps();
        self.ch1.sweep_tick_counter = self.ch1.sweep_tick_counter.saturating_add(1);
        if self.ch1.sweep_tick_counter < sweep_period {
            return;
        }
        self.ch1.sweep_tick_counter = 0;

        let delta = self.ch1.frequency_hz >> self.ch1.sweep_shift;
        let updated = if self.ch1.sweep_negate {
            self.ch1.frequency_hz.saturating_sub(delta)
        } else {
            self.ch1.frequency_hz.saturating_add(delta)
        };
        self.ch1.frequency_hz = updated.clamp(1, 20_000);
    }

    fn next_ch1_sample(&mut self) -> i16 {
        self.ch1_phase_accumulator =
            (self.ch1_phase_accumulator + self.ch1.frequency_hz) % Self::OUTPUT_SAMPLE_RATE_HZ;

        let duty_window = u32::from(self.ch1.duty.high_numerator())
            * (Self::OUTPUT_SAMPLE_RATE_HZ / u32::from(self.ch1.duty.denominator()));

        if self.ch1_phase_accumulator < duty_window {
            self.ch1.amplitude
        } else {
            -self.ch1.amplitude
        }
    }

    #[cfg(test)]
    fn set_ch1_sweep(&mut self, period_steps: u8, shift: u8, negate: bool) {
        self.ch1.sweep_period_steps = period_steps;
        self.ch1.sweep_shift = shift;
        self.ch1.sweep_negate = negate;
        self.ch1.sweep_tick_counter = 0;
    }

    #[cfg(test)]
    fn set_ch1_frequency_hz(&mut self, frequency_hz: u32) {
        self.ch1.frequency_hz = frequency_hz;
    }

    #[cfg(test)]
    fn ch1_frequency_hz(&self) -> u32 {
        self.ch1.frequency_hz
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
    use super::{Apu, DutyCycle};

    #[test]
    fn frame_sequencer_does_not_advance_before_threshold() { let mut apu = Apu::new(); assert_eq!(apu.tick(Apu::FRAME_SEQUENCER_PERIOD_T_CYCLES - 1),0); assert_eq!(apu.frame_step(),0);} 

    #[test]
    fn frame_sequencer_advances_once_at_threshold() { let mut apu = Apu::new(); assert_eq!(apu.tick(Apu::FRAME_SEQUENCER_PERIOD_T_CYCLES),1); assert_eq!(apu.frame_step(),1);} 

    #[test]
    fn frame_sequencer_wraps_after_eight_steps() { let mut apu=Apu::new(); let adv=apu.tick(Apu::FRAME_SEQUENCER_PERIOD_T_CYCLES*u32::from(Apu::FRAME_SEQUENCER_STEPS)); assert_eq!(adv,u32::from(Apu::FRAME_SEQUENCER_STEPS)); assert_eq!(apu.frame_step(),0);} 

    #[test]
    fn sample_generation_preserves_fractional_remainder_across_ticks() { let mut apu=Apu::new(); let _=apu.tick(87); assert_eq!(apu.queued_samples(),0); let _=apu.tick(1); assert_eq!(apu.queued_samples(),1);} 

    #[test]
    fn ch1_sweep_increases_frequency_on_sweep_ticks() {
        let mut apu = Apu::new();
        apu.set_ch1_frequency_hz(440);
        apu.set_ch1_sweep(1, 1, false);
        let _ = apu.tick(Apu::FRAME_SEQUENCER_PERIOD_T_CYCLES * 2); // step 2
        assert_eq!(apu.ch1_frequency_hz(), 660);
    }

    #[test]
    fn ch1_sweep_decreases_frequency_when_negated() {
        let mut apu = Apu::new();
        apu.set_ch1_frequency_hz(440);
        apu.set_ch1_sweep(1, 2, true);
        let _ = apu.tick(Apu::FRAME_SEQUENCER_PERIOD_T_CYCLES * 2);
        assert_eq!(apu.ch1_frequency_hz(), 330);
    }

    #[test]
    fn ch1_sweep_period_zero_maps_to_eight_steps() {
        let mut apu = Apu::new();
        apu.set_ch1_frequency_hz(440);
        apu.set_ch1_sweep(0, 1, false);

        let _ = apu.tick(Apu::FRAME_SEQUENCER_PERIOD_T_CYCLES * 26); // 7 sweep ticks (steps 2/6)
        assert_eq!(apu.ch1_frequency_hz(), 440);

        let _ = apu.tick(Apu::FRAME_SEQUENCER_PERIOD_T_CYCLES * 4); // 8th sweep tick
        assert_eq!(apu.ch1_frequency_hz(), 660);
    }

    #[test]
    fn ch1_generates_square_wave_with_non_silent_samples() {
        let mut apu = Apu::new();
        apu.ch1.duty = DutyCycle::Duty50;
        let _ = apu.tick(4_194);
        let samples = apu.drain_samples();
        assert!(!samples.is_empty());
        assert!(samples.iter().all(|sample| sample.abs() == 2_500));
    }
}
