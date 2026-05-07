use bitflags::bitflags;

/// Approximate DMG APU frame sequencer plus initial audio sample buffering.
///
/// This milestone provides deterministic timing plus a configurable Channel 1 square
/// wave path with basic sweep handling (`NR10` + `NR13/NR14`-style frequency updates).

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Nr10(u8);

impl From<u8> for Nr10 {
    fn from(value: u8) -> Self {
        Self(value)
    }
}

impl From<Nr10> for u8 {
    fn from(value: Nr10) -> Self {
        value.read_bits()
    }
}

impl Nr10 {
    pub const fn read_bits(self) -> u8 {
        self.0
    }

    pub const fn sweep_period_steps(self) -> u8 {
        (self.0 >> 4) & 0x07
    }

    pub const fn sweep_negate(self) -> bool {
        (self.0 & 0x08) != 0
    }

    pub const fn sweep_shift(self) -> u8 {
        self.0 & 0x07
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Nr50(u8);

impl From<u8> for Nr50 {
    fn from(value: u8) -> Self {
        Self(value)
    }
}

impl From<Nr50> for u8 {
    fn from(value: Nr50) -> Self {
        value.read_bits()
    }
}

impl Nr50 {
    pub const fn read_bits(self) -> u8 {
        self.0
    }

    pub fn write_bits(&mut self, value: u8) {
        self.0 = value;
    }

    pub const fn left_volume(self) -> u8 {
        (self.0 >> 4) & 0x07
    }

    pub const fn right_volume(self) -> u8 {
        self.0 & 0x07
    }

    pub const fn output_volume(self, is_left: bool) -> u8 {
        if is_left {
            self.left_volume()
        } else {
            self.right_volume()
        }
    }
}

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    pub struct Nr51: u8 {
        const CH1_RIGHT = 0x01;
        const CH2_RIGHT = 0x02;
        const CH3_RIGHT = 0x04;
        const CH4_RIGHT = 0x08;
        const CH1_LEFT = 0x10;
        const CH2_LEFT = 0x20;
        const CH3_LEFT = 0x40;
        const CH4_LEFT = 0x80;
    }
}

impl Nr51 {
    pub const fn read_bits(self) -> u8 {
        self.bits()
    }

    pub fn write_bits(&mut self, value: u8) {
        *self = Self::from_bits_retain(value);
    }

    pub const fn routes_channel_to_left(self, channel_index: u8) -> bool {
        (self.bits() & (0x10 << (channel_index & 0x03))) != 0
    }

    pub const fn routes_channel_to_right(self, channel_index: u8) -> bool {
        (self.bits() & (0x01 << (channel_index & 0x03))) != 0
    }

    pub const fn routes_channel(self, is_left: bool, channel_index: u8) -> bool {
        if is_left {
            self.routes_channel_to_left(channel_index)
        } else {
            self.routes_channel_to_right(channel_index)
        }
    }
}

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    pub struct Nr52: u8 {
        const CH1_ON = 0x01;
        const CH2_ON = 0x02;
        const CH3_ON = 0x04;
        const CH4_ON = 0x08;
        const POWER = 0x80;
    }
}

impl Nr52 {
    const READ_RESERVED: u8 = 0x70;

    pub const fn read_bits(self) -> u8 {
        self.bits()
    }

    pub fn write_power_bits(&mut self, value: u8) {
        *self = Self::from_bits_retain(value & Self::POWER.bits());
    }

    pub const fn power_enabled(self) -> bool {
        self.contains(Self::POWER)
    }

    pub const fn read_with_channel_status(self, status: Self) -> u8 {
        (self.bits() & Self::POWER.bits()) | status.bits() | Self::READ_RESERVED
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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
    registers: [u8; Self::REGISTER_COUNT],
    wave_ram_bytes: [u8; Self::WAVE_RAM_BYTE_COUNT],
    ch1: Ch1,
    ch2: Ch2,
    ch3: Ch3,
    ch4: Ch4,
    nr50: Nr50,
    nr51: Nr51,
    nr52: Nr52,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Ch2 {
    frequency_hz: u32,
    duty: DutyCycle,
    amplitude: i16,
    enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Ch3 {
    frequency_hz: u32,
    output_level_shift: u8,
    amplitude: i16,
    enabled: bool,
    wave_ram: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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
    pub const OUTPUT_QUEUE_TARGET_SAMPLES: usize = 2_048;
    pub const OUTPUT_QUEUE_MAX_SAMPLES: usize = 4_096;

    const CH1_DEFAULT_FREQUENCY_HZ: u32 = 440;
    const CH2_DEFAULT_FREQUENCY_HZ: u32 = 220;
    const CH3_DEFAULT_FREQUENCY_HZ: u32 = 330;
    const CH4_DEFAULT_FREQUENCY_HZ: u32 = 1_024;
    const CH1_DEFAULT_AMPLITUDE: i16 = 1_250;
    const CH2_DEFAULT_AMPLITUDE: i16 = 1_250;
    const CH3_DEFAULT_AMPLITUDE: i16 = 1_250;
    const REGISTER_START: u16 = 0xFF10;
    const REGISTER_END: u16 = 0xFF26;
    const REGISTER_COUNT: usize = (Self::REGISTER_END - Self::REGISTER_START + 1) as usize;
    const WAVE_RAM_START: u16 = 0xFF30;
    const WAVE_RAM_END: u16 = 0xFF3F;
    const WAVE_RAM_BYTE_COUNT: usize = (Self::WAVE_RAM_END - Self::WAVE_RAM_START + 1) as usize;
    const PCM_UNITS_PER_ENVELOPE_STEP: i16 = 180;

    const fn initial_registers() -> [u8; Self::REGISTER_COUNT] {
        let mut registers = [0; Self::REGISTER_COUNT];
        registers[(0xFF10 - Self::REGISTER_START) as usize] = 0x80;
        registers[(0xFF11 - Self::REGISTER_START) as usize] = 0xBF;
        registers[(0xFF12 - Self::REGISTER_START) as usize] = 0xF3;
        registers[(0xFF14 - Self::REGISTER_START) as usize] = 0xBF;
        registers[(0xFF16 - Self::REGISTER_START) as usize] = 0x3F;
        registers[(0xFF17 - Self::REGISTER_START) as usize] = 0x00;
        registers[(0xFF19 - Self::REGISTER_START) as usize] = 0xBF;
        registers[(0xFF1A - Self::REGISTER_START) as usize] = 0x7F;
        registers[(0xFF1B - Self::REGISTER_START) as usize] = 0xFF;
        registers[(0xFF1C - Self::REGISTER_START) as usize] = 0x9F;
        registers[(0xFF1E - Self::REGISTER_START) as usize] = 0xBF;
        registers[(0xFF20 - Self::REGISTER_START) as usize] = 0xFF;
        registers[(0xFF21 - Self::REGISTER_START) as usize] = 0x00;
        registers[(0xFF22 - Self::REGISTER_START) as usize] = 0x00;
        registers[(0xFF23 - Self::REGISTER_START) as usize] = 0xBF;
        registers[(0xFF24 - Self::REGISTER_START) as usize] = 0x77;
        registers[(0xFF25 - Self::REGISTER_START) as usize] = 0xF3;
        registers[(0xFF26 - Self::REGISTER_START) as usize] = 0xF0;
        registers
    }

    const fn is_unmapped_register_hole(address: u16) -> bool {
        matches!(address, 0xFF15 | 0xFF1F)
    }

    const fn register_index(address: u16) -> Option<usize> {
        if address >= Self::REGISTER_START
            && address <= Self::REGISTER_END
            && !Self::is_unmapped_register_hole(address)
        {
            Some((address - Self::REGISTER_START) as usize)
        } else {
            None
        }
    }

    const fn wave_ram_index(address: u16) -> Option<usize> {
        if address >= Self::WAVE_RAM_START && address <= Self::WAVE_RAM_END {
            Some((address - Self::WAVE_RAM_START) as usize)
        } else {
            None
        }
    }

    fn store_register(&mut self, address: u16, value: u8) {
        if let Some(index) = Self::register_index(address) {
            self.registers[index] = value;
        }
    }

    fn register(&self, address: u16) -> u8 {
        Self::register_index(address)
            .and_then(|index| self.registers.get(index).copied())
            .unwrap_or(0xFF)
    }

    fn square_frequency_from_period(period: u16) -> u32 {
        let denominator = 2048_u32.saturating_sub(u32::from(period)).max(1);
        (131_072 / denominator).clamp(1, 20_000)
    }

    fn ch1_period(&self) -> u16 {
        u16::from(self.register(0xFF13)) | (u16::from(self.register(0xFF14) & 0x07) << 8)
    }

    fn ch2_period(&self) -> u16 {
        u16::from(self.register(0xFF18)) | (u16::from(self.register(0xFF19) & 0x07) << 8)
    }

    fn ch3_period(&self) -> u16 {
        u16::from(self.register(0xFF1D)) | (u16::from(self.register(0xFF1E) & 0x07) << 8)
    }

    fn amplitude_from_envelope(value: u8) -> i16 {
        i16::from(value >> 4) * Self::PCM_UNITS_PER_ENVELOPE_STEP
    }

    fn envelope_dac_enabled(value: u8) -> bool {
        (value & 0xF8) != 0
    }

    fn ch3_dac_enabled(&self) -> bool {
        (self.register(0xFF1A) & 0x80) != 0
    }

    fn retrigger_ch1(&mut self) {
        self.ch1.frequency_hz = Self::square_frequency_from_period(self.ch1_period());
        self.ch1.duty = DutyCycle::from_duty_bits(self.register(0xFF11) >> 6);
        self.ch1.amplitude = Self::amplitude_from_envelope(self.register(0xFF12));
        self.ch1.enabled = Self::envelope_dac_enabled(self.register(0xFF12));
        self.ch1_phase_accumulator = 0;
        self.ch1.sweep_tick_counter = 0;
    }

    fn retrigger_ch2(&mut self) {
        self.ch2.frequency_hz = Self::square_frequency_from_period(self.ch2_period());
        self.ch2.duty = DutyCycle::from_duty_bits(self.register(0xFF16) >> 6);
        self.ch2.amplitude = Self::amplitude_from_envelope(self.register(0xFF17));
        self.ch2.enabled = Self::envelope_dac_enabled(self.register(0xFF17));
        self.ch2_phase_accumulator = 0;
    }

    fn retrigger_ch3(&mut self) {
        let period = self.ch3_period();
        let denominator = 2048_u32.saturating_sub(u32::from(period)).max(1);
        self.ch3.frequency_hz = (65_536 / denominator).clamp(1, 20_000);
        self.ch3.output_level_shift = (self.register(0xFF1C) >> 5) & 0x03;
        self.ch3.amplitude = Self::PCM_UNITS_PER_ENVELOPE_STEP * 8;
        self.ch3.enabled = self.ch3_dac_enabled();
        self.ch3_phase_accumulator = 0;
        self.ch3_wave_index = 1;
    }

    fn retrigger_ch4(&mut self) {
        let nr22 = self.register(0xFF22);
        let divisor_code = u32::from(nr22 & 0x07);
        let divisor = if divisor_code == 0 {
            8
        } else {
            divisor_code * 16
        };
        let shift = u32::from((nr22 >> 4) & 0x0F);
        self.ch4.frequency_hz = (524_288_u32 / divisor / (1_u32 << shift.min(15))).clamp(1, 20_000);
        self.ch4.amplitude = Self::amplitude_from_envelope(self.register(0xFF21));
        self.ch4.enabled = Self::envelope_dac_enabled(self.register(0xFF21));
        self.ch4_phase_accumulator = 0;
        self.ch4_lfsr = 0x7FFF;
    }

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
            registers: Self::initial_registers(),
            wave_ram_bytes: [
                0x02, 0x46, 0x8A, 0xCE, 0xFD, 0xB9, 0x75, 0x31, 0x02, 0x46, 0x8A, 0xCE, 0xFD, 0xB9,
                0x75, 0x31,
            ],
            ch1: Ch1 {
                frequency_hz: Self::CH1_DEFAULT_FREQUENCY_HZ,
                duty: DutyCycle::from_duty_bits(0b10),
                amplitude: 0,
                enabled: false,
                sweep_period_steps: 0,
                sweep_shift: 0,
                sweep_negate: false,
                sweep_tick_counter: 0,
            },
            ch2: Ch2 {
                frequency_hz: Self::CH2_DEFAULT_FREQUENCY_HZ,
                duty: DutyCycle::from_duty_bits(0b10),
                amplitude: 0,
                enabled: false,
            },
            ch3: Ch3 {
                frequency_hz: Self::CH3_DEFAULT_FREQUENCY_HZ,
                output_level_shift: 1,
                amplitude: 0,
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
            nr50: Nr50(0x77),
            nr51: Nr51::from_bits_retain(0xF3),
            nr52: Nr52::from_bits_retain(0xF0),
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

        if self.sample_buffer.len() > Self::OUTPUT_QUEUE_MAX_SAMPLES {
            let overflow = self.sample_buffer.len() - Self::OUTPUT_QUEUE_MAX_SAMPLES;
            self.sample_buffer.drain(..overflow);
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
        if !self.apu_power_enabled() {
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
        let mut mixed = 0_i16;
        if self.nr51.routes_channel(is_left, 0) && ch1 != 0 {
            mixed = mixed.saturating_add(ch1);
        }
        if self.nr51.routes_channel(is_left, 1) && ch2 != 0 {
            mixed = mixed.saturating_add(ch2);
        }
        if self.nr51.routes_channel(is_left, 2) && ch3 != 0 {
            mixed = mixed.saturating_add(ch3);
        }
        if self.nr51.routes_channel(is_left, 3) && ch4 != 0 {
            mixed = mixed.saturating_add(ch4);
        }

        let active_channels = [
            self.nr51.routes_channel(is_left, 0) && ch1 != 0,
            self.nr51.routes_channel(is_left, 1) && ch2 != 0,
            self.nr51.routes_channel(is_left, 2) && ch3 != 0,
            self.nr51.routes_channel(is_left, 3) && ch4 != 0,
        ]
        .into_iter()
        .filter(|active| *active)
        .count()
        .max(1);
        mixed = (i32::from(mixed) / active_channels as i32) as i16;

        let volume = self.nr50.output_volume(is_left);

        let scaled = (i32::from(mixed) * i32::from(volume + 1)) / 8;
        scaled.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16
    }

    pub(crate) fn load_startup_register(&mut self, address: u16, value: u8) -> bool {
        if let Some(index) = Self::wave_ram_index(address) {
            self.wave_ram_bytes[index] = value;
            let sample_index = index * 2;
            self.ch3.wave_ram[sample_index] = value >> 4;
            self.ch3.wave_ram[sample_index + 1] = value & 0x0F;
            return true;
        }

        if !matches!(address, Self::REGISTER_START..=Self::REGISTER_END)
            || Self::is_unmapped_register_hole(address)
        {
            return false;
        }

        self.store_register(address, value);
        match address {
            0xFF10 => self.write_ch1_sweep_register(Nr10::from(value)),
            0xFF11 => self.ch1.duty = DutyCycle::from_duty_bits(value >> 6),
            0xFF16 => self.ch2.duty = DutyCycle::from_duty_bits(value >> 6),
            0xFF1C => self.ch3.output_level_shift = (value >> 5) & 0x03,
            0xFF24 => self.nr50.write_bits(value),
            0xFF25 => self.nr51.write_bits(value),
            0xFF26 => {
                self.nr52.write_power_bits(value);
                self.store_register(address, self.nr52.read_bits() | Nr52::READ_RESERVED);
                self.ch1.enabled = false;
                self.ch2.enabled = false;
                self.ch3.enabled = false;
                self.ch4.enabled = false;
            }
            _ => {}
        }

        true
    }

    pub fn read_register(&self, address: u16) -> Option<u8> {
        if let Some(index) = Self::wave_ram_index(address) {
            return Some(self.wave_ram_bytes[index]);
        }

        match address {
            0xFF26 => Some(if self.apu_power_enabled() {
                self.nr52
                    .read_with_channel_status(self.channel_status_flags())
            } else {
                self.nr52.read_with_channel_status(Nr52::empty())
            }),
            address if Self::is_unmapped_register_hole(address) => None,
            Self::REGISTER_START..=Self::REGISTER_END => Some(match address {
                0xFF24 => self.nr50.read_bits(),
                0xFF25 => self.nr51.read_bits(),
                _ => self.register(address),
            }),
            _ => None,
        }
    }

    pub fn write_register(&mut self, address: u16, value: u8) -> bool {
        if let Some(index) = Self::wave_ram_index(address) {
            self.wave_ram_bytes[index] = value;
            let sample_index = index * 2;
            self.ch3.wave_ram[sample_index] = value >> 4;
            self.ch3.wave_ram[sample_index + 1] = value & 0x0F;
            return true;
        }

        if address == 0xFF26 {
            let was_powered = self.apu_power_enabled();
            self.nr52.write_power_bits(value);
            self.store_register(address, self.nr52.read_bits() | Nr52::READ_RESERVED);
            if !was_powered && self.apu_power_enabled() {
                self.frame_step = 0;
                self.t_cycle_counter = 0;
            }
            if was_powered && !self.apu_power_enabled() {
                self.power_off_reset();
            }
            return true;
        }

        if !matches!(address, Self::REGISTER_START..=Self::REGISTER_END)
            || Self::is_unmapped_register_hole(address)
        {
            return false;
        }

        if !self.apu_power_enabled() {
            return true;
        }

        self.store_register(address, value);
        match address {
            0xFF10 => self.write_ch1_sweep_register(Nr10::from(value)),
            0xFF11 => self.ch1.duty = DutyCycle::from_duty_bits(value >> 6),
            0xFF12 => {
                self.ch1.amplitude = Self::amplitude_from_envelope(value);
                if !Self::envelope_dac_enabled(value) {
                    self.ch1.enabled = false;
                }
            }
            0xFF14 if (value & 0x80) != 0 => self.retrigger_ch1(),
            0xFF16 => self.ch2.duty = DutyCycle::from_duty_bits(value >> 6),
            0xFF17 => {
                self.ch2.amplitude = Self::amplitude_from_envelope(value);
                if !Self::envelope_dac_enabled(value) {
                    self.ch2.enabled = false;
                }
            }
            0xFF19 if (value & 0x80) != 0 => self.retrigger_ch2(),
            0xFF1A if !self.ch3_dac_enabled() => self.ch3.enabled = false,
            0xFF1C => self.ch3.output_level_shift = (value >> 5) & 0x03,
            0xFF1E if (value & 0x80) != 0 => self.retrigger_ch3(),
            0xFF21 => {
                self.ch4.amplitude = Self::amplitude_from_envelope(value);
                if !Self::envelope_dac_enabled(value) {
                    self.ch4.enabled = false;
                }
            }
            0xFF23 if (value & 0x80) != 0 => self.retrigger_ch4(),
            0xFF24 => self.nr50.write_bits(value),
            0xFF25 => self.nr51.write_bits(value),
            _ => {}
        }

        true
    }

    const fn apu_power_enabled(&self) -> bool {
        self.nr52.power_enabled()
    }

    fn channel_status_flags(&self) -> Nr52 {
        let mut status = Nr52::empty();
        status.set(Nr52::CH1_ON, self.ch1.enabled);
        status.set(Nr52::CH2_ON, self.ch2.enabled);
        status.set(Nr52::CH3_ON, self.ch3.enabled);
        status.set(Nr52::CH4_ON, self.ch4.enabled);
        status
    }

    fn power_off_reset(&mut self) {
        self.registers = [0; Self::REGISTER_COUNT];
        self.store_register(0xFF26, Nr52::READ_RESERVED);
        self.nr50.write_bits(0);
        self.nr51.write_bits(0);
        self.ch1.frequency_hz = Self::CH1_DEFAULT_FREQUENCY_HZ;
        self.ch1.duty = DutyCycle::from_duty_bits(0b10);
        self.ch1.amplitude = Self::CH1_DEFAULT_AMPLITUDE;
        self.ch1.enabled = false;
        self.ch2.frequency_hz = Self::CH2_DEFAULT_FREQUENCY_HZ;
        self.ch2.duty = DutyCycle::from_duty_bits(0b10);
        self.ch2.amplitude = Self::CH2_DEFAULT_AMPLITUDE;
        self.ch2.enabled = false;
        self.ch3.frequency_hz = Self::CH3_DEFAULT_FREQUENCY_HZ;
        self.ch3.output_level_shift = 1;
        self.ch3.amplitude = Self::CH3_DEFAULT_AMPLITUDE;
        self.ch3.enabled = false;
        self.ch4.frequency_hz = Self::CH4_DEFAULT_FREQUENCY_HZ;
        self.ch4.amplitude = 0;
        self.ch4.enabled = false;
        self.ch1_phase_accumulator = 0;
        self.ch1.sweep_period_steps = 0;
        self.ch1.sweep_shift = 0;
        self.ch1.sweep_negate = false;
        self.ch1.sweep_tick_counter = 0;
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

    pub fn write_ch1_sweep_register(&mut self, nr10: Nr10) {
        self.ch1.sweep_period_steps = nr10.sweep_period_steps();
        self.ch1.sweep_shift = nr10.sweep_shift();
        self.ch1.sweep_negate = nr10.sweep_negate();
    }

    #[cfg(test)]
    fn set_ch1_sweep(&mut self, period_steps: u8, shift: u8, negate: bool) {
        self.write_ch1_sweep_register(Nr10::from(
            (period_steps << 4) | (u8::from(negate) << 3) | shift,
        ));
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
        self.ch1.enabled = amplitude != 0;
    }

    pub fn set_ch1_enabled(&mut self, enabled: bool) {
        if enabled && self.ch1.amplitude == 0 {
            self.ch1.amplitude = Self::CH1_DEFAULT_AMPLITUDE;
        }
        self.ch1.enabled = enabled;
    }

    #[cfg(test)]
    fn set_ch2_frequency_hz(&mut self, frequency_hz: u32) {
        self.ch2.frequency_hz = frequency_hz;
    }
    pub fn set_ch2_enabled(&mut self, enabled: bool) {
        if enabled && self.ch2.amplitude == 0 {
            self.ch2.amplitude = Self::CH2_DEFAULT_AMPLITUDE;
        }
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
    pub fn set_ch3_enabled(&mut self, enabled: bool) {
        if enabled && self.ch3.amplitude == 0 {
            self.ch3.amplitude = Self::CH3_DEFAULT_AMPLITUDE;
        }
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

    /// Drain exactly `requested_samples` for an audio device callback.
    ///
    /// - If too many samples are queued, oldest samples are dropped first to keep
    ///   output latency bounded and avoid long-term A/V drift.
    /// - If too few samples are queued, output is padded with silence to avoid
    ///   callback underruns.
    pub fn pull_output_samples(&mut self, requested_samples: usize) -> Vec<i16> {
        if self.sample_buffer.len() > Self::OUTPUT_QUEUE_TARGET_SAMPLES {
            let keep = Self::OUTPUT_QUEUE_TARGET_SAMPLES.max(requested_samples);
            let drop = self.sample_buffer.len().saturating_sub(keep);
            if drop > 0 {
                self.sample_buffer.drain(..drop);
            }
        }

        let available = requested_samples.min(self.sample_buffer.len());
        let mut output: Vec<i16> = self.sample_buffer.drain(..available).collect();
        if output.len() < requested_samples {
            output.resize(requested_samples, 0);
        }
        output
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
    fn pull_output_samples_zero_pads_when_queue_is_short() {
        let mut apu = Apu::new();
        let out = apu.pull_output_samples(16);
        assert_eq!(out.len(), 16);
        assert!(out.iter().all(|sample| *sample == 0));
    }

    #[test]
    fn pull_output_samples_drops_oldest_samples_when_queue_is_too_deep() {
        let mut apu = Apu::new();
        apu.sample_buffer = (0..(Apu::OUTPUT_QUEUE_TARGET_SAMPLES + 10))
            .map(|i| i as i16)
            .collect();

        let out = apu.pull_output_samples(8);
        assert_eq!(out, vec![10, 11, 12, 13, 14, 15, 16, 17]);
    }

    #[test]
    fn sample_queue_is_hard_clamped_to_prevent_unbounded_growth() {
        let mut apu = Apu::new();
        let _ = apu.tick(Apu::DMG_CLOCK_HZ * 2);
        assert!(apu.queued_samples() <= Apu::OUTPUT_QUEUE_MAX_SAMPLES);
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
    fn unmapped_apu_register_holes_are_not_readable_or_writable() {
        let mut apu = Apu::new();

        for address in [0xFF15, 0xFF1F] {
            assert_eq!(apu.read_register(address), None);
            assert!(!apu.write_register(address, 0x5A));
            assert_eq!(apu.read_register(address), None);
        }
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
    fn nr52_read_low_bits_are_derived_from_live_channel_state() {
        let mut apu = Apu::new();
        apu.ch1.enabled = false;
        let nr52 = apu.read_register(0xFF26).unwrap_or(0);
        assert_eq!(nr52 & 0x01, 0);
    }

    #[test]
    fn nr52_ch4_status_bit_tracks_enable_even_with_zero_amplitude() {
        let mut apu = Apu::new();
        apu.set_ch4_amplitude(0);
        apu.set_ch4_enabled(true);
        let nr52 = apu.read_register(0xFF26).unwrap_or(0);
        assert_ne!(nr52 & 0x08, 0);
    }

    #[test]
    fn nr52_initializes_with_dmg_reset_upper_nibble() {
        let apu = Apu::new();
        let nr52 = apu.read_register(0xFF26).unwrap_or(0);
        assert_eq!(nr52 & 0xF0, 0xF0);
        assert_eq!(nr52 & 0x0F, 0);
    }

    #[test]
    fn nr52_power_reenable_resets_frame_sequencer_phase() {
        let mut apu = Apu::new();
        let _ = apu.tick(Apu::FRAME_SEQUENCER_PERIOD_T_CYCLES * 3);
        assert_ne!(apu.frame_step(), 0);

        assert!(apu.write_register(0xFF26, 0x00));
        assert!(apu.write_register(0xFF26, 0x80));
        assert_eq!(apu.frame_step(), 0);

        let advanced = apu.tick(Apu::FRAME_SEQUENCER_PERIOD_T_CYCLES);
        assert_eq!(advanced, 1);
        assert_eq!(apu.frame_step(), 1);
    }

    #[test]
    fn power_off_freezes_ch1_sweep_state_while_apu_is_disabled() {
        let mut apu = Apu::new();
        apu.set_ch1_frequency_hz(440);
        apu.set_ch1_sweep(1, 1, false);

        assert!(apu.write_register(0xFF26, 0x00));
        let _ = apu.tick(Apu::FRAME_SEQUENCER_PERIOD_T_CYCLES * 8);
        assert_eq!(apu.ch1_frequency_hz(), 440);
    }

    #[test]
    fn power_off_clears_channel_config_to_defaults() {
        let mut apu = Apu::new();
        apu.set_ch1_frequency_hz(2_000);
        apu.set_ch1_amplitude(50);
        apu.set_ch2_frequency_hz(1_333);
        apu.set_ch2_duty(DutyCycle::Duty75);
        apu.ch2.amplitude = 75;
        apu.ch3.frequency_hz = 999;
        apu.set_ch3_level_shift(3);
        apu.set_ch3_amplitude(42);
        apu.set_ch4_frequency_hz(777);
        apu.set_ch4_amplitude(24);

        assert!(apu.write_register(0xFF26, 0x00));

        assert_eq!(apu.ch1.frequency_hz, Apu::CH1_DEFAULT_FREQUENCY_HZ);
        assert_eq!(apu.ch1.amplitude, Apu::CH1_DEFAULT_AMPLITUDE);
        assert_eq!(apu.ch2.frequency_hz, Apu::CH2_DEFAULT_FREQUENCY_HZ);
        assert!(matches!(apu.ch2.duty, DutyCycle::Duty50));
        assert_eq!(apu.ch2.amplitude, Apu::CH2_DEFAULT_AMPLITUDE);
        assert_eq!(apu.ch3.frequency_hz, Apu::CH3_DEFAULT_FREQUENCY_HZ);
        assert_eq!(apu.ch3.output_level_shift, 1);
        assert_eq!(apu.ch3.amplitude, Apu::CH3_DEFAULT_AMPLITUDE);
        assert_eq!(apu.ch4.frequency_hz, Apu::CH4_DEFAULT_FREQUENCY_HZ);
        assert_eq!(apu.ch4.amplitude, 0);
    }

    #[test]
    fn ch1_can_be_reenabled_after_nr52_power_cycle() {
        let mut apu = Apu::new();
        assert!(apu.write_register(0xFF25, 0x11));
        assert!(apu.write_register(0xFF24, 0x77));

        assert!(apu.write_register(0xFF26, 0x00));
        assert!(apu.write_register(0xFF26, 0x80));
        assert!(apu.write_register(0xFF25, 0x11));
        assert!(apu.write_register(0xFF24, 0x77));
        apu.set_ch1_enabled(true);

        let _ = apu.tick(4_194);
        let samples = apu.drain_samples();
        assert!(!samples.is_empty());
        assert!(samples.iter().any(|sample| *sample != 0));
    }

    #[test]
    fn nr12_zero_disables_ch1_instead_of_falling_back_to_default_tone() {
        let mut apu = Apu::new();
        assert!(apu.write_register(0xFF25, 0x11));
        assert!(apu.write_register(0xFF12, 0x00));
        assert!(apu.write_register(0xFF14, 0x80));

        let _ = apu.tick(4_194);
        let samples = apu.drain_samples();
        assert!(!samples.is_empty());
        assert!(samples.iter().all(|sample| *sample == 0));
        assert_eq!(apu.read_register(0xFF26).unwrap_or(0) & 0x01, 0);
    }

    #[test]
    fn ch2_trigger_uses_mmio_frequency_and_envelope_registers() {
        let mut low = Apu::new();
        assert!(low.write_register(0xFF25, 0x22));
        assert!(low.write_register(0xFF16, 0x80));
        assert!(low.write_register(0xFF17, 0xF0));
        assert!(low.write_register(0xFF18, 0x00));
        assert!(low.write_register(0xFF19, 0x80));
        let _ = low.tick(4_194);
        let low_samples = low.drain_samples();

        let mut high = Apu::new();
        assert!(high.write_register(0xFF25, 0x22));
        assert!(high.write_register(0xFF16, 0x80));
        assert!(high.write_register(0xFF17, 0xF0));
        assert!(high.write_register(0xFF18, 0xFF));
        assert!(high.write_register(0xFF19, 0x87));
        let _ = high.tick(4_194);
        let high_samples = high.drain_samples();

        assert!(!low_samples.is_empty());
        assert!(!high_samples.is_empty());
        assert_ne!(low_samples, high_samples);
        assert!(high_samples.iter().map(|s| s.abs()).max().unwrap_or(0) > 2_000);
    }

    #[test]
    fn direct_ch3_enable_restores_default_amplitude() {
        let mut apu = Apu::new();
        apu.set_ch1_amplitude(0);
        apu.ch2.amplitude = 0;
        apu.set_ch3_wave_ram([15; 32]);
        apu.set_ch3_enabled(true);

        let _ = apu.tick(400);
        let samples = apu.drain_samples();

        assert!(!samples.is_empty());
        assert!(samples.iter().any(|sample| *sample != 0));
    }

    #[test]
    fn mmio_triggered_ch3_starts_from_lower_ff30_nibble() {
        let mut apu = Apu::new();
        assert!(apu.write_register(0xFF30, 0x0F));
        for address in 0xFF31..=0xFF3F {
            assert!(apu.write_register(address, 0x00));
        }
        assert!(apu.write_register(0xFF25, 0x44));
        assert!(apu.write_register(0xFF1A, 0x80));
        assert!(apu.write_register(0xFF1C, 0x20));
        assert!(apu.write_register(0xFF1D, 0x00));
        assert!(apu.write_register(0xFF1E, 0x80));

        let _ = apu.tick(88);
        let samples = apu.drain_samples();

        assert_eq!(samples.len(), 1);
        assert!(samples[0] > 0);
    }

    #[test]
    fn wave_ram_writes_feed_ch3_samples() {
        let mut apu = Apu::new();
        for address in 0xFF30..=0xFF3F {
            assert!(apu.write_register(address, 0xFF));
        }
        assert!(apu.write_register(0xFF25, 0x44));
        assert!(apu.write_register(0xFF1A, 0x80));
        assert!(apu.write_register(0xFF1C, 0x20));
        assert!(apu.write_register(0xFF1D, 0xFF));
        assert!(apu.write_register(0xFF1E, 0x87));

        let _ = apu.tick(4_194);
        let samples = apu.drain_samples();
        assert!(!samples.is_empty());
        assert!(samples.iter().all(|sample| *sample > 0));
    }
}
