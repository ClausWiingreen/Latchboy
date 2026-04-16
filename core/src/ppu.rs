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
const INTERRUPT_VBLANK_BIT: u8 = 0x01;
const INTERRUPT_STAT_BIT: u8 = 0x02;
const INTERRUPT_ENABLE_VBLANK_BIT: u8 = 0x01;
const INTERRUPT_ENABLE_STAT_BIT: u8 = 0x02;

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
        }
    }
}

impl Ppu {
    fn current_mode(&self) -> u8 {
        self.stat & STAT_MODE_MASK
    }

    fn set_mode(&mut self, mode: u8) {
        self.stat = (self.stat & !STAT_MODE_MASK) | (mode & STAT_MODE_MASK);
    }

    fn update_lyc_coincidence_and_request_interrupt(
        &mut self,
        interrupt_flag: &mut u8,
        previous_coincidence: bool,
    ) {
        let now_coincident = self.ly == self.lyc;
        if now_coincident {
            self.stat |= STAT_LYC_EQUAL_BIT;
        } else {
            self.stat &= !STAT_LYC_EQUAL_BIT;
        }

        if !previous_coincidence
            && now_coincident
            && (self.stat & STAT_COINCIDENCE_INTERRUPT_BIT) != 0
        {
            *interrupt_flag |= INTERRUPT_STAT_BIT;
        }
    }

    fn request_mode_interrupt_if_enabled(&self, mode: u8, interrupt_flag: &mut u8) {
        let interrupt_enabled = match mode {
            0 => (self.stat & STAT_MODE_0_INTERRUPT_BIT) != 0,
            1 => (self.stat & STAT_MODE_1_INTERRUPT_BIT) != 0,
            2 => (self.stat & STAT_MODE_2_INTERRUPT_BIT) != 0,
            _ => false,
        };

        if interrupt_enabled {
            *interrupt_flag |= INTERRUPT_STAT_BIT;
        }
    }

    fn set_mode_with_interrupts(&mut self, mode: u8, previous_mode: u8, interrupt_flag: &mut u8) {
        self.set_mode(mode);
        if mode != previous_mode {
            self.request_mode_interrupt_if_enabled(mode, interrupt_flag);
        }
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
            LCDC_REGISTER => self.lcdc = value,
            STAT_REGISTER => {
                let readonly_bits = self.stat & 0x07;
                self.stat = 0x80 | readonly_bits | (value & 0x78);
            }
            SCY_REGISTER => self.scy = value,
            SCX_REGISTER => self.scx = value,
            LY_REGISTER => self.ly = 0,
            LYC_REGISTER => self.lyc = value,
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

    pub fn step(&mut self, interrupt_flag: &mut u8) {
        if (self.lcdc & LCDC_ENABLED_BIT) == 0 {
            self.scanline_dot = 0;
            self.ly = 0;
            self.set_mode(0);
            self.stat &= !STAT_LYC_EQUAL_BIT;
            return;
        }

        let previous_mode = self.current_mode();
        let previous_coincidence = (self.stat & STAT_LYC_EQUAL_BIT) != 0;

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

        self.set_mode_with_interrupts(next_mode, previous_mode, interrupt_flag);
        self.update_lyc_coincidence_and_request_interrupt(interrupt_flag, previous_coincidence);
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
        ppu.write_register(LCDC_REGISTER, 0x91);
        ppu.write_register(SCY_REGISTER, 0x56);
        ppu.write_register(BGP_REGISTER, 0xFC);

        assert_eq!(ppu.read_vram(0x8000), 0x12);
        assert_eq!(ppu.read_oam(0xFE00), 0x34);
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
}
