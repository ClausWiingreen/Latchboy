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
        }
    }
}

impl Ppu {
    pub fn read_vram(&self, address: u16) -> u8 {
        self.vram[(address - VRAM_START) as usize]
    }

    pub fn write_vram(&mut self, address: u16, value: u8) {
        self.vram[(address - VRAM_START) as usize] = value;
    }

    pub fn read_oam(&self, address: u16) -> u8 {
        self.oam[(address - OAM_START) as usize]
    }

    pub fn write_oam(&mut self, address: u16, value: u8) {
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
}
