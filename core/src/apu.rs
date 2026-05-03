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
    nr50: u8,
    nr51: u8,
    nr52: u8,
}

#[derive(Debug, Clone)]
struct Ch1 {
    frequency_hz: u32,
    duty: DutyCycle,
    amplitude: i16,
    enabled: bool,
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
    enabled: bool,
}

#[derive(Debug, Clone)]
struct Ch3 {
    frequency_hz: u32,
    output_level_shift: u8,
    amplitude: i16,
    enabled: bool,
    wave_ram: [u8; 32],
}

#[derive(Debug, Clone)]
struct Ch4 {
    frequency_hz: u32,
    amplitude: i16,
    enabled: bool,
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
                enabled: true,
                sweep_period_steps: 0,
                sweep_shift: 0,
                sweep_negate: false,
                sweep_tick_counter: 0,
            },
            ch2: Ch2 {
                frequency_hz: Self::CH2_DEFAULT_FREQUENCY_HZ,
                duty: DutyCycle::from_duty_bits(0b10),
                amplitude: Self::CH2_DEFAULT_AMPLITUDE,
                enabled: false,
            },
            ch3: Ch3 {
                frequency_hz: Self::CH3_DEFAULT_FREQUENCY_HZ,
                output_level_shift: 1,
                amplitude: 1_250,
                enabled: false,
                wave_ram: [
                    0, 2, 4, 6, 8, 10, 12, 14, 15, 13, 11, 9, 7, 5, 3, 1, 0, 2, 4, 6, 8, 10, 12,
                    14, 15, 13, 11, 9, 7, 5, 3, 1,
                ],
            },
            ch4: Ch4 {
                frequency_hz: Self::CH4_DEFAULT_FREQUENCY_HZ,
                amplitude: 0,
                enabled: false,
            },
            nr50: 0x77,
            nr51: 0xF3,
            nr52: 0xF1,
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
        if self.nr52 & 0x80 == 0 {
            return 0;
        }

        let ch1 = self.next_ch1_sample();
        let ch2 = self.next_ch2_sample();
        let ch3 = self.next_ch3_sample();
        let ch4 = if self.ch4.enabled {
            self.next_ch4_sample()
        } else {
            0
        };

        let left = self.mix_stereo_side(true, ch1, ch2, ch3, ch4);
        let right = self.mix_stereo_side(false, ch1, ch2, ch3, ch4);

        ((i32::from(left) + i32::from(right)) / 2) as i16
    }

    fn mix_stereo_side(&self, is_left: bool, ch1: i16, ch2: i16, ch3: i16, ch4: i16) -> i16 {
        let routing_shift = if is_left { 4 } else { 0 };
        let channel_mask = (self.nr51 >> routing_shift) & 0x0F;
        let mut mixed = 0_i16;
        if channel_mask & 0x01 != 0 {
            mixed = mixed.saturating_add(ch1);
        }
        if channel_mask & 0x02 != 0 {
            mixed = mixed.saturating_add(ch2);
        }
        if channel_mask & 0x04 != 0 {
            mixed = mixed.saturating_add(ch3);
        }
        if channel_mask & 0x08 != 0 {
            mixed = mixed.saturating_add(ch4);
        }

        let volume = if is_left {
            (self.nr50 >> 4) & 0x07
        } else {
            self.nr50 & 0x07
        };

        let scaled = (i32::from(mixed) * i32::from(volume + 1)) / 8;
        scaled.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16
    }

    pub fn read_register(&self, address: u16) -> Option<u8> {
        match address {
            0xFF24 => Some(self.nr50),
            0xFF25 => Some(self.nr51),
            0xFF26 => Some(if self.apu_power_enabled() {
                self.nr52 | self.channel_status_flags() | 0x70
            } else {
                self.nr52 | 0x70
            }),
            _ => None,
        }
    }

    pub fn write_register(&mut self, address: u16, value: u8) -> bool {
        match address {
            0xFF24 => {
                if self.apu_power_enabled() {
                    self.nr50 = value;
                }
                true
            }
            0xFF25 => {
                if self.apu_power_enabled() {
                    self.nr51 = value;
                }
                true
            }
            0xFF26 => {
                let was_powered = self.apu_power_enabled();
                self.nr52 = value & 0x80;
                if was_powered && !self.apu_power_enabled() {
                    self.power_off_reset();
                }
                true
            }
            _ => false,
        }
    }

    const fn apu_power_enabled(&self) -> bool {
        self.nr52 & 0x80 != 0
    }

    fn channel_status_flags(&self) -> u8 {
        let ch1 = u8::from(self.ch1.enabled);
        let ch2 = u8::from(self.ch2.enabled) << 1;
        let ch3 = u8::from(self.ch3.enabled) << 2;
        let ch4 = u8::from(self.ch4.enabled && self.ch4.amplitude != 0) << 3;
        ch1 | ch2 | ch3 | ch4
    }

    fn power_off_reset(&mut self) {
        self.nr50 = 0;
        self.nr51 = 0;
        self.ch1.enabled = false;
        self.ch2.enabled = false;
        self.ch3.enabled = false;
        self.ch4.enabled = false;
        self.ch1_phase_accumulator = 0;
        self.ch2_phase_accumulator = 0;
        self.ch3_phase_accumulator = 0;
        self.ch3_wave_index = 0;
        self.ch4_phase_accumulator = 0;
        self.ch4_lfsr = 0x7FFF;
    }

    fn next_ch1_sample(&mut self) -> i16 {
        if !self.ch1.enabled {
            return 0;
        }
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
        if !self.ch2.enabled {
            return 0;
        }
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
        if !self.ch3.enabled {
            return 0;
        }
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
        self.ch4_phase_accumulator = self
            .ch4_phase_accumulator
            .saturating_add(self.ch4.frequency_hz);
        let step_width = Self::OUTPUT_SAMPLE_RATE_HZ;
        while self.ch4_phase_accumulator >= step_width {
            self.ch4_phase_accumulator -= step_width;
            let feedback = (self.ch4_lfsr ^ (self.ch4_lfsr >> 1)) & 1;
            self.ch4_lfsr = (self.ch4_lfsr >> 1) | (feedback << 14);
        }
        self.ch4_phase_accumulator %= Self::OUTPUT_SAMPLE_RATE_HZ;

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
    fn set_ch2_enabled(&mut self, enabled: bool) {
        self.ch2.enabled = enabled;
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
    fn set_ch3_enabled(&mut self, enabled: bool) {
        self.ch3.enabled = enabled;
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

    #[cfg(test)]
    fn set_ch4_enabled(&mut self, enabled: bool) {
        self.ch4.enabled = enabled;
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
        apu.set_ch2_enabled(true);
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
        apu.set_ch3_enabled(true);
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
        apu.set_ch4_frequency_hz(Apu::OUTPUT_SAMPLE_RATE_HZ);
        apu.set_ch4_enabled(true);
        assert!(apu.write_register(0xFF25, 0x88));

        let _ = apu.tick(Apu::DMG_CLOCK_HZ / 5);
        let samples = apu.drain_samples();
        assert!(!samples.is_empty());
        assert!(samples.iter().any(|s| *s > 0));
        assert!(samples.iter().any(|s| *s < 0));
        assert!(samples.iter().all(|s| s.abs() == 700));
    }

    #[test]
    fn ch4_is_muted_by_default_and_does_not_affect_existing_mix() {
        let mut apu = Apu::new();
        apu.ch1.duty = DutyCycle::Duty50;
        apu.set_ch1_amplitude(0);
        apu.set_ch2_frequency_hz(220);
        apu.set_ch2_duty(DutyCycle::Duty25);
        apu.set_ch2_enabled(true);
        apu.set_ch3_level_shift(0);
        let _ = apu.tick(4_194);
        let samples = apu.drain_samples();
        assert!(!samples.is_empty());
        assert!(samples.iter().all(|sample| sample.abs() == 1_250));
    }

    #[test]
    fn nr52_master_enable_mutes_all_output_when_disabled() {
        let mut apu = Apu::new();
        assert!(apu.write_register(0xFF26, 0x00));
        let _ = apu.tick(4_194);
        let samples = apu.drain_samples();
        assert!(!samples.is_empty());
        assert!(samples.iter().all(|sample| *sample == 0));
    }

    #[test]
    fn nr51_channel_routing_can_isolate_single_channel() {
        let mut apu = Apu::new();
        apu.set_ch1_amplitude(900);
        apu.ch2.amplitude = 0;
        apu.set_ch3_amplitude(0);
        apu.set_ch4_amplitude(0);
        assert!(apu.write_register(0xFF25, 0x11));
        assert!(apu.write_register(0xFF24, 0x77));

        let _ = apu.tick(4_194);
        let samples = apu.drain_samples();
        assert!(!samples.is_empty());
        assert!(samples.iter().all(|sample| sample.abs() == 900));
    }

    #[test]
    fn nr50_volume_scales_output_amplitude() {
        let mut apu = Apu::new();
        apu.set_ch1_amplitude(800);
        apu.ch2.amplitude = 0;
        apu.set_ch3_amplitude(0);
        apu.set_ch4_amplitude(0);
        assert!(apu.write_register(0xFF25, 0x11));
        assert!(apu.write_register(0xFF24, 0x00));
        let _ = apu.tick(4_194);
        let quiet_peak = apu
            .drain_samples()
            .iter()
            .map(|s| s.abs())
            .max()
            .unwrap_or(0);

        assert!(apu.write_register(0xFF24, 0x77));
        let _ = apu.tick(4_194);
        let loud_peak = apu
            .drain_samples()
            .iter()
            .map(|s| s.abs())
            .max()
            .unwrap_or(0);
        assert!(loud_peak > quiet_peak);
    }

    #[test]
    fn nr52_read_clears_channel_status_bits_when_powered_off() {
        let mut apu = Apu::new();
        apu.set_ch1_amplitude(500);
        apu.ch2.amplitude = 300;
        apu.set_ch3_amplitude(400);
        apu.set_ch4_amplitude(200);
        apu.set_ch4_enabled(true);

        assert!(apu.write_register(0xFF26, 0x00));
        assert_eq!(apu.read_register(0xFF26), Some(0x70));
    }

    #[test]
    fn nr50_nr51_writes_are_ignored_while_powered_off() {
        let mut apu = Apu::new();
        assert!(apu.write_register(0xFF24, 0x12));
        assert!(apu.write_register(0xFF25, 0x34));
        assert_eq!(apu.read_register(0xFF24), Some(0x12));
        assert_eq!(apu.read_register(0xFF25), Some(0x34));

        assert!(apu.write_register(0xFF26, 0x00));
        assert!(apu.write_register(0xFF24, 0xAB));
        assert!(apu.write_register(0xFF25, 0xCD));

        assert_eq!(apu.read_register(0xFF24), Some(0x00));
        assert_eq!(apu.read_register(0xFF25), Some(0x00));
    }

    #[test]
    fn nr52_read_keeps_reserved_bits_set_when_powered_on_or_off() {
        let mut apu = Apu::new();
        let powered_on = apu.read_register(0xFF26).unwrap_or(0);
        assert_eq!(powered_on & 0x70, 0x70);

        assert!(apu.write_register(0xFF26, 0x00));
        let powered_off = apu.read_register(0xFF26).unwrap_or(0);
        assert_eq!(powered_off & 0x70, 0x70);
    }

    #[test]
    fn nr52_initializes_with_dmg_reset_upper_nibble() {
        let apu = Apu::new();
        let nr52 = apu.read_register(0xFF26).unwrap_or(0);
        assert_eq!(nr52 & 0xF0, 0xF0);
        assert_ne!(nr52 & 0x01, 0);
    }
}
