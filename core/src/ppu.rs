pub const LCDC_REGISTER: u16 = 0xFF40;
pub const STAT_REGISTER: u16 = 0xFF41;
pub const SCY_REGISTER: u16 = 0xFF42;
pub const SCX_REGISTER: u16 = 0xFF43;
pub const LY_REGISTER: u16 = 0xFF44;
pub const LYC_REGISTER: u16 = 0xFF45;
pub const DMA_REGISTER: u16 = 0xFF46;
pub const BGP_REGISTER: u16 = 0xFF47;
pub const OBP0_REGISTER: u16 = 0xFF48;
pub const OBP1_REGISTER: u16 = 0xFF49;
pub const WY_REGISTER: u16 = 0xFF4A;
pub const WX_REGISTER: u16 = 0xFF4B;

const VRAM_START: u16 = 0x8000;
const OAM_START: u16 = 0xFE00;

const VRAM_SIZE: usize = 0x2000;
const OAM_SIZE: usize = 0xA0;
const CYCLES_PER_SCANLINE: u16 = 456;
const VISIBLE_SCANLINES: u8 = 144;
const TOTAL_SCANLINES: u8 = 154;
const MODE2_CYCLES: u16 = 80;
const MODE3_CYCLES: u16 = 172;
const MODE0_CYCLES_END: u16 = MODE2_CYCLES + MODE3_CYCLES;

const STAT_COINCIDENCE_INTERRUPT_BIT: u8 = 0x40;
const STAT_MODE_2_INTERRUPT_BIT: u8 = 0x20;
const STAT_MODE_1_INTERRUPT_BIT: u8 = 0x10;
const STAT_MODE_0_INTERRUPT_BIT: u8 = 0x08;
const STAT_LYC_EQUAL_BIT: u8 = 0x04;

const STAT_MODE_MASK: u8 = 0x03;
const LCDC_ENABLED_BIT: u8 = 0x80;
const LCDC_BG_ENABLE_BIT: u8 = 0x01;
const LCDC_BG_TILE_MAP_SELECT_BIT: u8 = 0x08;
const LCDC_BG_TILE_DATA_SELECT_BIT: u8 = 0x10;
const INTERRUPT_VBLANK_BIT: u8 = 0x01;
const INTERRUPT_STAT_BIT: u8 = 0x02;
const INTERRUPT_ENABLE_VBLANK_BIT: u8 = 0x01;
const INTERRUPT_ENABLE_STAT_BIT: u8 = 0x02;

const BG_MAP_0_OFFSET: usize = 0x1800; // 0x9800-0x9BFF
const BG_MAP_1_OFFSET: usize = 0x1C00; // 0x9C00-0x9FFF
const TILE_BLOCK_0_OFFSET: usize = 0x0000; // 0x8000-0x87FF
const TILE_BLOCK_2_OFFSET: usize = 0x1000; // 0x9000-0x97FF

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Ppu {
    vram: [u8; VRAM_SIZE],
    oam: [u8; OAM_SIZE],
    lcdc: u8,
    stat: u8,
    scy: u8,
    scx: u8,
    ly: u8,
    lyc: u8,
    dma: u8,
    bgp: u8,
    obp0: u8,
    obp1: u8,
    wy: u8,
    wx: u8,
    scanline_dot: u16,
    stat_irq_line_high: bool,
    stat_irq_pending: bool,
}

impl Default for Ppu {
    fn default() -> Self {
        Self {
            vram: [0; VRAM_SIZE],
            oam: [0; OAM_SIZE],
            lcdc: 0,
            stat: 0x80,
            scy: 0,
            scx: 0,
            ly: 0,
            lyc: 0,
            dma: 0,
            bgp: 0,
            obp0: 0,
            obp1: 0,
            wy: 0,
            wx: 0,
            scanline_dot: 0,
            stat_irq_line_high: false,
            stat_irq_pending: false,
        }
    }
}

impl Ppu {
    fn bg_map_base_offset(&self) -> usize {
        if (self.lcdc & LCDC_BG_TILE_MAP_SELECT_BIT) != 0 {
            BG_MAP_1_OFFSET
        } else {
            BG_MAP_0_OFFSET
        }
    }

    fn tile_data_row_offset(&self, tile_index: u8, row_in_tile: u8) -> usize {
        let row_base = usize::from(row_in_tile) * 2;
        if (self.lcdc & LCDC_BG_TILE_DATA_SELECT_BIT) != 0 {
            TILE_BLOCK_0_OFFSET + usize::from(tile_index) * 16 + row_base
        } else {
            let signed_index = i8::from_ne_bytes([tile_index]);
            let tile_offset = (isize::from(signed_index) * 16) as isize;
            (TILE_BLOCK_2_OFFSET as isize + tile_offset + row_base as isize) as usize
        }
    }

    fn current_mode(&self) -> u8 {
        self.stat & STAT_MODE_MASK
    }

    fn set_mode(&mut self, mode: u8) {
        self.stat = (self.stat & !STAT_MODE_MASK) | (mode & STAT_MODE_MASK);
    }

    fn update_lyc_coincidence_flag(&mut self) {
        if self.ly == self.lyc {
            self.stat |= STAT_LYC_EQUAL_BIT;
        } else {
            self.stat &= !STAT_LYC_EQUAL_BIT;
        }
    }

    fn stat_irq_condition_active(&self) -> bool {
        if (self.lcdc & LCDC_ENABLED_BIT) == 0 {
            return false;
        }

        let mode = self.current_mode();
        let mode_enabled = (mode == 0 && (self.stat & STAT_MODE_0_INTERRUPT_BIT) != 0)
            || (mode == 1 && (self.stat & STAT_MODE_1_INTERRUPT_BIT) != 0)
            || (mode == 2 && (self.stat & STAT_MODE_2_INTERRUPT_BIT) != 0);
        let coincidence_enabled_and_true = (self.stat & STAT_COINCIDENCE_INTERRUPT_BIT) != 0
            && (self.stat & STAT_LYC_EQUAL_BIT) != 0;

        mode_enabled || coincidence_enabled_and_true
    }

    fn update_stat_irq_line(&mut self, interrupt_flag: Option<&mut u8>) {
        let next_line_high = self.stat_irq_condition_active();
        if !self.stat_irq_line_high && next_line_high {
            if let Some(flag) = interrupt_flag {
                *flag |= INTERRUPT_STAT_BIT;
            } else {
                self.stat_irq_pending = true;
            }
        }
        self.stat_irq_line_high = next_line_high;
    }

    fn vram_accessible(&self) -> bool {
        self.current_mode() != 0x03
    }

    fn oam_accessible(&self) -> bool {
        !matches!(self.current_mode(), 0x02 | 0x03)
    }

    pub fn may_request_interrupt(&self, interrupt_enable: u8) -> bool {
        if (self.lcdc & LCDC_ENABLED_BIT) == 0 {
            return false;
        }

        if (interrupt_enable & INTERRUPT_ENABLE_VBLANK_BIT) != 0 {
            return true;
        }

        if (interrupt_enable & INTERRUPT_ENABLE_STAT_BIT) == 0 {
            return false;
        }

        if (self.stat
            & (STAT_MODE_0_INTERRUPT_BIT | STAT_MODE_1_INTERRUPT_BIT | STAT_MODE_2_INTERRUPT_BIT))
            != 0
        {
            return true;
        }

        (self.stat & STAT_COINCIDENCE_INTERRUPT_BIT) != 0 && self.lyc < TOTAL_SCANLINES
    }

    pub fn read_vram(&self, address: u16) -> u8 {
        if !self.vram_accessible() {
            return 0xFF;
        }

        self.vram[(address - VRAM_START) as usize]
    }

    pub fn write_vram(&mut self, address: u16, value: u8) {
        if !self.vram_accessible() {
            return;
        }

        self.vram[(address - VRAM_START) as usize] = value;
    }

    pub fn read_oam(&self, address: u16) -> u8 {
        if !self.oam_accessible() {
            return 0xFF;
        }

        self.oam[(address - OAM_START) as usize]
    }

    pub fn write_oam(&mut self, address: u16, value: u8) {
        if !self.oam_accessible() {
            return;
        }

        self.oam[(address - OAM_START) as usize] = value;
    }

    pub fn read_register(&self, address: u16) -> Option<u8> {
        let value = match address {
            LCDC_REGISTER => self.lcdc,
            STAT_REGISTER => self.stat | 0x80,
            SCY_REGISTER => self.scy,
            SCX_REGISTER => self.scx,
            LY_REGISTER => self.ly,
            LYC_REGISTER => self.lyc,
            DMA_REGISTER => self.dma,
            BGP_REGISTER => self.bgp,
            OBP0_REGISTER => self.obp0,
            OBP1_REGISTER => self.obp1,
            WY_REGISTER => self.wy,
            WX_REGISTER => self.wx,
            _ => return None,
        };

        Some(value)
    }

    pub fn write_register(&mut self, address: u16, value: u8) -> bool {
        match address {
            LCDC_REGISTER => {
                let was_enabled = (self.lcdc & LCDC_ENABLED_BIT) != 0;
                self.lcdc = value;
                let now_enabled = (self.lcdc & LCDC_ENABLED_BIT) != 0;

                if !was_enabled && now_enabled {
                    self.scanline_dot = 0;
                    self.ly = 0;
                    self.set_mode(0x02);
                    self.update_lyc_coincidence_flag();
                } else if was_enabled && !now_enabled {
                    self.scanline_dot = 0;
                    self.ly = 0;
                    self.set_mode(0x00);
                    self.update_lyc_coincidence_flag();
                }

                self.update_stat_irq_line(None);
            }
            STAT_REGISTER => {
                let readonly_bits = self.stat & 0x07;
                self.stat = 0x80 | readonly_bits | (value & 0x78);
                self.update_stat_irq_line(None);
            }
            SCY_REGISTER => self.scy = value,
            SCX_REGISTER => self.scx = value,
            LY_REGISTER => {
                self.ly = 0;
                self.update_lyc_coincidence_flag();
                self.update_stat_irq_line(None);
            }
            LYC_REGISTER => {
                self.lyc = value;
                self.update_lyc_coincidence_flag();
                self.update_stat_irq_line(None);
            }
            DMA_REGISTER => self.dma = value,
            BGP_REGISTER => self.bgp = value,
            OBP0_REGISTER => self.obp0 = value,
            OBP1_REGISTER => self.obp1 = value,
            WY_REGISTER => self.wy = value,
            WX_REGISTER => self.wx = value,
            _ => return false,
        }

        true
    }

    /// Returns the 2-bit DMG background color index (0-3) for the given screen pixel.
    ///
    /// This method implements the Milestone 4 background tile fetch + map addressing path:
    /// - Selects tile map base from LCDC bit 3 (`0x9800` vs `0x9C00`).
    /// - Selects tile data addressing mode from LCDC bit 4 (unsigned `0x8000` region or
    ///   signed indexing around `0x9000`).
    /// - Applies scroll offsets using `SCX/SCY`.
    pub fn background_pixel_color_id(&self, screen_x: u8, screen_y: u8) -> u8 {
        if (self.lcdc & LCDC_BG_ENABLE_BIT) == 0 {
            return 0;
        }

        let bg_x = screen_x.wrapping_add(self.scx);
        let bg_y = screen_y.wrapping_add(self.scy);
        let tile_col = (bg_x / 8) as usize;
        let tile_row = (bg_y / 8) as usize;
        let row_in_tile = bg_y % 8;
        let pixel_in_tile = 7 - (bg_x % 8);

        let map_index = tile_row * 32 + tile_col;
        let tile_index = self.vram[self.bg_map_base_offset() + map_index];
        let tile_row_offset = self.tile_data_row_offset(tile_index, row_in_tile);
        let low = self.vram[tile_row_offset];
        let high = self.vram[tile_row_offset + 1];

        let low_bit = (low >> pixel_in_tile) & 0x01;
        let high_bit = (high >> pixel_in_tile) & 0x01;
        (high_bit << 1) | low_bit
    }

    pub fn take_stat_irq_pending(&mut self) -> bool {
        let pending = self.stat_irq_pending;
        self.stat_irq_pending = false;
        pending
    }

    pub fn step(&mut self, interrupt_flag: &mut u8) {
        if (self.lcdc & LCDC_ENABLED_BIT) == 0 {
            self.scanline_dot = 0;
            self.ly = 0;
            self.set_mode(0);
            self.update_lyc_coincidence_flag();
            self.update_stat_irq_line(Some(interrupt_flag));
            return;
        }

        let previous_mode = self.current_mode();

        self.scanline_dot = self.scanline_dot.wrapping_add(1);
        if self.scanline_dot >= CYCLES_PER_SCANLINE {
            self.scanline_dot = 0;
            self.ly = (self.ly + 1) % TOTAL_SCANLINES;
        }

        let next_mode = if self.ly >= VISIBLE_SCANLINES {
            1
        } else if self.scanline_dot < MODE2_CYCLES {
            2
        } else if self.scanline_dot < MODE0_CYCLES_END {
            3
        } else {
            0
        };

        if previous_mode != 1 && next_mode == 1 {
            *interrupt_flag |= INTERRUPT_VBLANK_BIT;
        }

        self.set_mode(next_mode);
        self.update_lyc_coincidence_flag();
        self.update_stat_irq_line(Some(interrupt_flag));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ppu_maps_vram_oam_and_lcd_registers() {
        let mut ppu = Ppu::default();

        ppu.write_vram(0x8000, 0x12);
        ppu.write_oam(0xFE00, 0x34);

        assert_eq!(ppu.read_vram(0x8000), 0x12);
        assert_eq!(ppu.read_oam(0xFE00), 0x34);

        ppu.write_register(LCDC_REGISTER, 0x91);
        ppu.write_register(SCY_REGISTER, 0x56);
        ppu.write_register(BGP_REGISTER, 0xFC);

        assert_eq!(ppu.read_register(LCDC_REGISTER), Some(0x91));
        assert_eq!(ppu.read_register(SCY_REGISTER), Some(0x56));
        assert_eq!(ppu.read_register(BGP_REGISTER), Some(0xFC));
    }

    #[test]
    fn stat_and_ly_register_writes_follow_hardware_constraints() {
        let mut ppu = Ppu::default();

        ppu.write_register(STAT_REGISTER, 0xFF);
        assert_eq!(ppu.read_register(STAT_REGISTER), Some(0xF8));

        ppu.write_register(LY_REGISTER, 0x99);
        assert_eq!(ppu.read_register(LY_REGISTER), Some(0x00));
    }

    #[test]
    fn vram_access_is_blocked_during_mode_3() {
        let mut ppu = Ppu::default();

        ppu.write_vram(0x8000, 0x12);
        assert_eq!(ppu.read_vram(0x8000), 0x12);

        ppu.stat = (ppu.stat & !0x03) | 0x03;
        assert_eq!(ppu.read_vram(0x8000), 0xFF);

        ppu.write_vram(0x8000, 0x34);
        ppu.stat &= !0x03;
        assert_eq!(ppu.read_vram(0x8000), 0x12);
    }

    #[test]
    fn oam_access_is_blocked_during_modes_2_and_3() {
        let mut ppu = Ppu::default();

        ppu.write_oam(0xFE00, 0x56);
        assert_eq!(ppu.read_oam(0xFE00), 0x56);

        ppu.stat = (ppu.stat & !0x03) | 0x02;
        assert_eq!(ppu.read_oam(0xFE00), 0xFF);
        ppu.write_oam(0xFE00, 0x78);

        ppu.stat = (ppu.stat & !0x03) | 0x03;
        assert_eq!(ppu.read_oam(0xFE00), 0xFF);

        ppu.stat &= !0x03;
        assert_eq!(ppu.read_oam(0xFE00), 0x56);
    }

    #[test]
    fn step_transitions_through_scanline_modes() {
        let mut ppu = Ppu::default();
        let mut interrupt_flag = 0u8;
        ppu.write_register(LCDC_REGISTER, 0x80);

        ppu.step(&mut interrupt_flag);
        assert_eq!(ppu.read_register(STAT_REGISTER).unwrap() & 0x03, 0x02);
        assert_eq!(ppu.read_register(LY_REGISTER), Some(0x00));

        for _ in 1..MODE2_CYCLES {
            ppu.step(&mut interrupt_flag);
        }
        assert_eq!(ppu.read_register(STAT_REGISTER).unwrap() & 0x03, 0x03);

        for _ in MODE2_CYCLES..MODE0_CYCLES_END {
            ppu.step(&mut interrupt_flag);
        }
        assert_eq!(ppu.read_register(STAT_REGISTER).unwrap() & 0x03, 0x00);

        for _ in MODE0_CYCLES_END..CYCLES_PER_SCANLINE {
            ppu.step(&mut interrupt_flag);
        }
        assert_eq!(ppu.read_register(LY_REGISTER), Some(0x01));
        assert_eq!(ppu.read_register(STAT_REGISTER).unwrap() & 0x03, 0x02);
    }

    #[test]
    fn step_enters_vblank_and_requests_vblank_and_stat_interrupts() {
        let mut ppu = Ppu::default();
        let mut interrupt_flag = 0u8;
        ppu.write_register(LCDC_REGISTER, 0x80);
        ppu.write_register(STAT_REGISTER, STAT_MODE_1_INTERRUPT_BIT);

        let cycles_to_vblank = (CYCLES_PER_SCANLINE as u32) * (VISIBLE_SCANLINES as u32);
        for _ in 0..cycles_to_vblank {
            ppu.step(&mut interrupt_flag);
        }

        assert_eq!(ppu.read_register(LY_REGISTER), Some(VISIBLE_SCANLINES));
        assert_eq!(ppu.read_register(STAT_REGISTER).unwrap() & 0x03, 0x01);
        assert_eq!(interrupt_flag & INTERRUPT_VBLANK_BIT, INTERRUPT_VBLANK_BIT);
        assert_eq!(interrupt_flag & INTERRUPT_STAT_BIT, INTERRUPT_STAT_BIT);
    }

    #[test]
    fn lyc_match_sets_stat_coincidence_and_requests_stat_interrupt() {
        let mut ppu = Ppu::default();
        let mut interrupt_flag = 0u8;
        ppu.write_register(LCDC_REGISTER, 0x80);
        ppu.write_register(LYC_REGISTER, 0x01);
        ppu.write_register(STAT_REGISTER, STAT_COINCIDENCE_INTERRUPT_BIT);

        for _ in 0..CYCLES_PER_SCANLINE {
            ppu.step(&mut interrupt_flag);
        }

        assert_eq!(ppu.read_register(LY_REGISTER), Some(0x01));
        assert_eq!(
            ppu.read_register(STAT_REGISTER).unwrap() & STAT_LYC_EQUAL_BIT,
            STAT_LYC_EQUAL_BIT
        );
        assert_eq!(interrupt_flag & INTERRUPT_STAT_BIT, INTERRUPT_STAT_BIT);
    }

    #[test]
    fn may_request_interrupt_reflects_lcdc_ie_and_stat_sources() {
        let mut ppu = Ppu::default();
        ppu.write_register(LCDC_REGISTER, 0x80);

        assert!(ppu.may_request_interrupt(INTERRUPT_ENABLE_VBLANK_BIT));
        assert!(!ppu.may_request_interrupt(0x00));

        ppu.write_register(STAT_REGISTER, STAT_MODE_2_INTERRUPT_BIT);
        assert!(ppu.may_request_interrupt(INTERRUPT_ENABLE_STAT_BIT));

        ppu.write_register(STAT_REGISTER, STAT_COINCIDENCE_INTERRUPT_BIT);
        ppu.write_register(LYC_REGISTER, 153);
        assert!(ppu.may_request_interrupt(INTERRUPT_ENABLE_STAT_BIT));

        ppu.write_register(LYC_REGISTER, 200);
        assert!(!ppu.may_request_interrupt(INTERRUPT_ENABLE_STAT_BIT));

        ppu.write_register(LCDC_REGISTER, 0x00);
        assert!(!ppu.may_request_interrupt(INTERRUPT_ENABLE_VBLANK_BIT));
    }

    #[test]
    fn enabling_mode_source_while_mode_is_active_queues_stat_interrupt() {
        let mut ppu = Ppu::default();
        let mut interrupt_flag = 0u8;
        ppu.write_register(LCDC_REGISTER, 0x80);
        ppu.step(&mut interrupt_flag);
        assert_eq!(ppu.read_register(STAT_REGISTER).unwrap() & 0x03, 0x02);
        assert!(!ppu.take_stat_irq_pending());

        ppu.write_register(STAT_REGISTER, STAT_MODE_2_INTERRUPT_BIT);
        assert!(ppu.take_stat_irq_pending());
    }

    #[test]
    fn enabling_or_matching_coincidence_condition_queues_stat_interrupt() {
        let mut ppu = Ppu::default();
        ppu.write_register(LCDC_REGISTER, 0x80);
        ppu.write_register(LYC_REGISTER, 0x00);
        assert_eq!(
            ppu.read_register(STAT_REGISTER).unwrap() & STAT_LYC_EQUAL_BIT,
            STAT_LYC_EQUAL_BIT
        );

        ppu.write_register(STAT_REGISTER, STAT_COINCIDENCE_INTERRUPT_BIT);
        assert!(ppu.take_stat_irq_pending());

        let mut ppu = Ppu::default();
        ppu.write_register(LCDC_REGISTER, 0x80);
        ppu.write_register(LYC_REGISTER, 0x01);
        ppu.write_register(STAT_REGISTER, STAT_COINCIDENCE_INTERRUPT_BIT);
        assert!(!ppu.take_stat_irq_pending());
        ppu.write_register(LYC_REGISTER, 0x00);
        assert!(ppu.take_stat_irq_pending());
    }

    #[test]
    fn lcd_enable_enters_mode_2_before_stat_eval() {
        let mut ppu = Ppu::default();
        ppu.write_register(STAT_REGISTER, STAT_MODE_0_INTERRUPT_BIT);
        assert!(!ppu.take_stat_irq_pending());

        ppu.write_register(LCDC_REGISTER, 0x80);

        assert_eq!(ppu.read_register(STAT_REGISTER).unwrap() & 0x03, 0x02);
        assert!(!ppu.take_stat_irq_pending());
    }

    #[test]
    fn lcd_enable_can_immediately_raise_mode_2_stat_when_enabled() {
        let mut ppu = Ppu::default();
        ppu.write_register(STAT_REGISTER, STAT_MODE_2_INTERRUPT_BIT);
        assert!(!ppu.take_stat_irq_pending());

        ppu.write_register(LCDC_REGISTER, 0x80);

        assert_eq!(ppu.read_register(STAT_REGISTER).unwrap() & 0x03, 0x02);
        assert!(ppu.take_stat_irq_pending());
    }

    #[test]
    fn stat_line_handoff_between_sources_does_not_create_spurious_edge() {
        let mut ppu = Ppu::default();
        let mut interrupt_flag = 0u8;
        ppu.write_register(LCDC_REGISTER, 0x80);
        ppu.write_register(LYC_REGISTER, 0x01);
        ppu.write_register(
            STAT_REGISTER,
            STAT_MODE_0_INTERRUPT_BIT | STAT_COINCIDENCE_INTERRUPT_BIT,
        );

        for _ in 0..(CYCLES_PER_SCANLINE - 1) {
            ppu.step(&mut interrupt_flag);
        }
        assert_eq!(ppu.read_register(LY_REGISTER), Some(0x00));
        assert_eq!(ppu.read_register(STAT_REGISTER).unwrap() & 0x03, 0x00);

        interrupt_flag = 0;
        ppu.step(&mut interrupt_flag);

        assert_eq!(ppu.read_register(LY_REGISTER), Some(0x01));
        assert_eq!(ppu.read_register(STAT_REGISTER).unwrap() & 0x03, 0x02);
        assert_eq!(
            ppu.read_register(STAT_REGISTER).unwrap() & STAT_LYC_EQUAL_BIT,
            STAT_LYC_EQUAL_BIT
        );
        assert_eq!(interrupt_flag & INTERRUPT_STAT_BIT, 0);
    }

    #[test]
    fn background_pixel_fetch_uses_unsigned_tile_data_region() {
        let mut ppu = Ppu::default();
        ppu.write_register(
            LCDC_REGISTER,
            LCDC_BG_ENABLE_BIT | LCDC_BG_TILE_DATA_SELECT_BIT,
        );

        ppu.write_vram(0x9800, 0x02);
        ppu.write_vram(0x8020, 0b1000_0000);
        ppu.write_vram(0x8021, 0b1000_0000);

        assert_eq!(ppu.background_pixel_color_id(0, 0), 3);
    }

    #[test]
    fn background_pixel_fetch_uses_signed_tile_data_region() {
        let mut ppu = Ppu::default();
        ppu.write_register(LCDC_REGISTER, LCDC_BG_ENABLE_BIT);

        ppu.write_vram(0x9800, 0xFF);
        ppu.write_vram(0x8FF0, 0b1000_0000);
        ppu.write_vram(0x8FF1, 0b0000_0000);

        assert_eq!(ppu.background_pixel_color_id(0, 0), 1);
    }

    #[test]
    fn background_pixel_fetch_selects_background_map_and_applies_scroll() {
        let mut ppu = Ppu::default();
        ppu.write_register(
            LCDC_REGISTER,
            LCDC_BG_ENABLE_BIT | LCDC_BG_TILE_MAP_SELECT_BIT | LCDC_BG_TILE_DATA_SELECT_BIT,
        );
        ppu.write_register(SCX_REGISTER, 8);
        ppu.write_register(SCY_REGISTER, 16);

        ppu.write_vram(0x9C41, 0x03);
        ppu.write_vram(0x8030, 0b0000_0000);
        ppu.write_vram(0x8031, 0b1000_0000);

        assert_eq!(ppu.background_pixel_color_id(0, 0), 2);
    }
}
