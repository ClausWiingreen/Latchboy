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
    ch2_phase_accumulator: u32,
    ch3_phase_accumulator: u32,
    ch3_wave_index: u8,
    ch4_phase_accumulator: u32,
    ch4_lfsr: u16,
    ch1: Ch1,
    ch2: Ch2,
    ch3: Ch3,
    ch4: Ch4,
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
    const fn from_duty_bits(bits: u8) -> Self {
        match bits & 0b11 {
            0b00 => Self::Duty12_5,
            0b01 => Self::Duty25,
            0b10 => Self::Duty50,
            0b11 => Self::Duty75,
            _ => Self::Duty50,
        }
    }

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

#[derive(Debug, Clone)]
struct Ch2 {
    frequency_hz: u32,
    duty: DutyCycle,
    amplitude: i16,
}

#[derive(Debug, Clone)]
struct Ch3 {
    frequency_hz: u32,
    output_level_shift: u8,
    amplitude: i16,
    wave_ram: [u8; 32],
}

#[derive(Debug, Clone)]
struct Ch4 {
    frequency_hz: u32,
    amplitude: i16,
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
    const CH2_DEFAULT_FREQUENCY_HZ: u32 = 220;
    const CH3_DEFAULT_FREQUENCY_HZ: u32 = 330;
    const CH4_DEFAULT_FREQUENCY_HZ: u32 = 1_024;
    const CH1_DEFAULT_AMPLITUDE: i16 = 1_250;
    const CH2_DEFAULT_AMPLITUDE: i16 = 1_250;

    #[must_use]
    pub const fn new() -> Self {
        Self {
            frame_step: 0,
            t_cycle_counter: 0,
            sample_phase: 0,
            sample_buffer: Vec::new(),
            ch1_phase_accumulator: 0,
            ch2_phase_accumulator: 0,
            ch3_phase_accumulator: 0,
            ch3_wave_index: 0,
            ch4_phase_accumulator: 0,
            ch4_lfsr: 0x7FFF,
            ch1: Ch1 {
                frequency_hz: Self::CH1_DEFAULT_FREQUENCY_HZ,
                duty: DutyCycle::from_duty_bits(0b10),
                amplitude: Self::CH1_DEFAULT_AMPLITUDE,
                sweep_period_steps: 0,
                sweep_shift: 0,
                sweep_negate: false,
                sweep_tick_counter: 0,
            },
            ch2: Ch2 {
                frequency_hz: Self::CH2_DEFAULT_FREQUENCY_HZ,
                duty: DutyCycle::from_duty_bits(0b10),
                amplitude: Self::CH2_DEFAULT_AMPLITUDE,
            },
            ch3: Ch3 {
                frequency_hz: Self::CH3_DEFAULT_FREQUENCY_HZ,
                output_level_shift: 1,
                amplitude: 1_250,
                wave_ram: [
                    0, 2, 4, 6, 8, 10, 12, 14, 15, 13, 11, 9, 7, 5, 3, 1, 0, 2, 4, 6, 8, 10, 12,
                    14, 15, 13, 11, 9, 7, 5, 3, 1,
                ],
            },
            ch4: Ch4 {
                frequency_hz: Self::CH4_DEFAULT_FREQUENCY_HZ,
                amplitude: 900,
            },
        }
    }

    #[must_use]
    pub fn tick(&mut self, t_cycles: u32) -> u32 {
        let mut remaining_t_cycles = t_cycles;
        let mut advanced_steps = 0_u32;

        while remaining_t_cycles > 0 {
            let until_frame_step =
                Self::FRAME_SEQUENCER_PERIOD_T_CYCLES.saturating_sub(self.t_cycle_counter);
            let segment_t_cycles = remaining_t_cycles.min(until_frame_step);

            self.emit_ch1_samples_for_t_cycles(segment_t_cycles);
            self.t_cycle_counter += segment_t_cycles;
            remaining_t_cycles -= segment_t_cycles;

            if self.t_cycle_counter == Self::FRAME_SEQUENCER_PERIOD_T_CYCLES {
                self.t_cycle_counter = 0;
                self.frame_step = (self.frame_step + 1) % Self::FRAME_SEQUENCER_STEPS;
                self.apply_ch1_sweep_on_frame_step();
                advanced_steps += 1;
            }
        }

        advanced_steps
    }

    fn emit_ch1_samples_for_t_cycles(&mut self, t_cycles: u32) {
        self.sample_phase += u64::from(t_cycles) * u64::from(Self::OUTPUT_SAMPLE_RATE_HZ);
        let generated_samples = self.sample_phase / u64::from(Self::DMG_CLOCK_HZ);
        self.sample_phase %= u64::from(Self::DMG_CLOCK_HZ);
        for _ in 0..generated_samples {
            let sample = self.next_mixed_sample();
            self.sample_buffer.push(sample);
        }
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

    fn next_mixed_sample(&mut self) -> i16 {
        let ch1 = self.next_ch1_sample();
        let ch2 = self.next_ch2_sample();
        let ch3 = self.next_ch3_sample();
        let ch4 = self.next_ch4_sample();
        ch1.saturating_add(ch2)
            .saturating_add(ch3)
            .saturating_add(ch4)
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

    fn next_ch2_sample(&mut self) -> i16 {
        self.ch2_phase_accumulator =
            (self.ch2_phase_accumulator + self.ch2.frequency_hz) % Self::OUTPUT_SAMPLE_RATE_HZ;

        let duty_window = u32::from(self.ch2.duty.high_numerator())
            * (Self::OUTPUT_SAMPLE_RATE_HZ / u32::from(self.ch2.duty.denominator()));

        if self.ch2_phase_accumulator < duty_window {
            self.ch2.amplitude
        } else {
            -self.ch2.amplitude
        }
    }

    fn next_ch3_sample(&mut self) -> i16 {
        self.ch3_phase_accumulator =
            (self.ch3_phase_accumulator + self.ch3.frequency_hz) % Self::OUTPUT_SAMPLE_RATE_HZ;

        let step_width = Self::OUTPUT_SAMPLE_RATE_HZ / 32;
        while self.ch3_phase_accumulator >= step_width {
            self.ch3_phase_accumulator -= step_width;
            self.ch3_wave_index = (self.ch3_wave_index + 1) % 32;
        }

        let raw = i16::from(self.ch3.wave_ram[usize::from(self.ch3_wave_index)]) - 8;
        let shifted = match self.ch3.output_level_shift {
            0 => 0,
            1 => raw,
            2 => raw / 2,
            3 => raw / 4,
            _ => raw,
        };
        shifted.saturating_mul(self.ch3.amplitude / 8)
    }

    fn next_ch4_sample(&mut self) -> i16 {
        self.ch4_phase_accumulator =
            (self.ch4_phase_accumulator + self.ch4.frequency_hz) % Self::OUTPUT_SAMPLE_RATE_HZ;

        let step_width = Self::OUTPUT_SAMPLE_RATE_HZ / 64;
        while self.ch4_phase_accumulator >= step_width {
            self.ch4_phase_accumulator -= step_width;
            let feedback = (self.ch4_lfsr ^ (self.ch4_lfsr >> 1)) & 1;
            self.ch4_lfsr = (self.ch4_lfsr >> 1) | (feedback << 14);
        }

        if self.ch4_lfsr & 1 == 0 {
            self.ch4.amplitude
        } else {
            -self.ch4.amplitude
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

    #[cfg(test)]
    fn set_ch1_amplitude(&mut self, amplitude: i16) {
        self.ch1.amplitude = amplitude;
    }

    #[cfg(test)]
    fn set_ch2_frequency_hz(&mut self, frequency_hz: u32) {
        self.ch2.frequency_hz = frequency_hz;
    }

    #[cfg(test)]
    fn set_ch2_duty(&mut self, duty: DutyCycle) {
        self.ch2.duty = duty;
    }

    #[cfg(test)]
    fn set_ch3_level_shift(&mut self, output_level_shift: u8) {
        self.ch3.output_level_shift = output_level_shift;
    }

    #[cfg(test)]
    fn set_ch3_amplitude(&mut self, amplitude: i16) {
        self.ch3.amplitude = amplitude;
    }

    #[cfg(test)]
    fn set_ch3_wave_ram(&mut self, wave_ram: [u8; 32]) {
        self.ch3.wave_ram = wave_ram;
        self.ch3_wave_index = 0;
        self.ch3_phase_accumulator = 0;
    }

    #[cfg(test)]
    fn set_ch4_frequency_hz(&mut self, frequency_hz: u32) {
        self.ch4.frequency_hz = frequency_hz;
    }

    #[cfg(test)]
    fn set_ch4_amplitude(&mut self, amplitude: i16) {
        self.ch4.amplitude = amplitude;
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
    fn frame_sequencer_does_not_advance_before_threshold() {
        let mut apu = Apu::new();
        assert_eq!(apu.tick(Apu::FRAME_SEQUENCER_PERIOD_T_CYCLES - 1), 0);
        assert_eq!(apu.frame_step(), 0);
    }

    #[test]
    fn frame_sequencer_advances_once_at_threshold() {
        let mut apu = Apu::new();
        assert_eq!(apu.tick(Apu::FRAME_SEQUENCER_PERIOD_T_CYCLES), 1);
        assert_eq!(apu.frame_step(), 1);
    }

    #[test]
    fn frame_sequencer_wraps_after_eight_steps() {
        let mut apu = Apu::new();
        let adv =
            apu.tick(Apu::FRAME_SEQUENCER_PERIOD_T_CYCLES * u32::from(Apu::FRAME_SEQUENCER_STEPS));
        assert_eq!(adv, u32::from(Apu::FRAME_SEQUENCER_STEPS));
        assert_eq!(apu.frame_step(), 0);
    }

    #[test]
    fn sample_generation_preserves_fractional_remainder_across_ticks() {
        let mut apu = Apu::new();
        let _ = apu.tick(87);
        assert_eq!(apu.queued_samples(), 0);
        let _ = apu.tick(1);
        assert_eq!(apu.queued_samples(), 1);
    }

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
    fn sweep_timing_is_chunk_size_independent_for_sample_emission() {
        let mut small_ticks = Apu::new();
        small_ticks.set_ch1_frequency_hz(440);
        small_ticks.set_ch1_sweep(1, 1, false);

        let mut large_tick = small_ticks.clone();
        let total_cycles = Apu::FRAME_SEQUENCER_PERIOD_T_CYCLES * 4;

        for _ in 0..total_cycles {
            let _ = small_ticks.tick(1);
        }
        let _ = large_tick.tick(total_cycles);

        assert_eq!(small_ticks.drain_samples(), large_tick.drain_samples());
    }

    #[test]
    fn mixed_ch1_ch2_output_contains_expected_amplitudes() {
        let mut apu = Apu::new();
        apu.ch1.duty = DutyCycle::Duty50;
        apu.set_ch1_amplitude(0);
        apu.set_ch2_frequency_hz(220);
        apu.set_ch2_duty(DutyCycle::Duty25);
        apu.set_ch3_level_shift(0);
        let _ = apu.tick(4_194);
        let samples = apu.drain_samples();
        assert!(!samples.is_empty());
        assert!(samples.iter().all(|sample| sample.abs() == 1_250));
    }

    #[test]
    fn ch3_wave_channel_obeys_output_level_control() {
        let mut apu = Apu::new();
        apu.set_ch1_amplitude(0);
        apu.ch2.amplitude = 0;
        apu.set_ch3_amplitude(800);
        apu.set_ch3_wave_ram([15; 32]);

        apu.set_ch3_level_shift(1);
        let _ = apu.tick(400);
        let full = apu.drain_samples();
        assert!(!full.is_empty());
        let full_peak = full.iter().map(|s| s.abs()).max().unwrap_or(0);

        apu.set_ch3_level_shift(3);
        let _ = apu.tick(400);
        let quarter = apu.drain_samples();
        assert!(!quarter.is_empty());
        let quarter_peak = quarter.iter().map(|s| s.abs()).max().unwrap_or(0);

        assert!(quarter_peak < full_peak);
    }

    #[test]
    fn ch4_noise_channel_emits_bipolar_noise() {
        let mut apu = Apu::new();
        apu.set_ch1_amplitude(0);
        apu.ch2.amplitude = 0;
        apu.set_ch3_amplitude(0);
        apu.set_ch4_amplitude(700);
        apu.set_ch4_frequency_hz(2_048);

        let _ = apu.tick(4_194);
        let samples = apu.drain_samples();
        assert!(!samples.is_empty());
        assert!(samples.iter().any(|s| *s > 0));
        assert!(samples.iter().any(|s| *s < 0));
        assert!(samples.iter().all(|s| s.abs() == 700));
    }
}
