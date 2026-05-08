use bitflags::bitflags;

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
pub const VBK_REGISTER: u16 = 0xFF4F;
pub const BCPS_REGISTER: u16 = 0xFF68;
pub const BCPD_REGISTER: u16 = 0xFF69;
pub const OCPS_REGISTER: u16 = 0xFF6A;
pub const OCPD_REGISTER: u16 = 0xFF6B;

const VRAM_START: u16 = 0x8000;
const OAM_START: u16 = 0xFE00;

const VRAM_BANK_SIZE: usize = 0x2000;
const VRAM_BANK_COUNT: usize = 2;
const VRAM_SIZE: usize = VRAM_BANK_SIZE * VRAM_BANK_COUNT;
const CGB_PALETTE_RAM_SIZE: usize = 0x40;
const CGB_PALETTE_INDEX_MASK: u8 = 0x3F;
const CGB_PALETTE_AUTO_INCREMENT: u8 = 0x80;
const CGB_PALETTE_INDEX_READ_MASK: u8 = 0x40;
const CGB_VBK_READ_MASK: u8 = 0xFE;
const OAM_SIZE: usize = 0xA0;
const OAM_ENTRY_SIZE: usize = 4;
const OAM_SPRITE_COUNT: usize = OAM_SIZE / OAM_ENTRY_SIZE;
const MAX_SPRITES_PER_SCANLINE: usize = 10;
const CYCLES_PER_SCANLINE: u16 = 456;
const VISIBLE_SCANLINES: u8 = 144;
const TOTAL_SCANLINES: u8 = 154;
const MODE2_CYCLES: u16 = 80;
const MODE3_CYCLES: u16 = 172;
const MODE0_CYCLES_END: u16 = MODE2_CYCLES + MODE3_CYCLES;
pub(crate) const LCD_ENABLE_STARTUP_DELAY_DOTS: u8 = 16;
pub const FRAMEBUFFER_WIDTH: usize = 160;
pub const FRAMEBUFFER_HEIGHT: usize = 144;
pub const FRAMEBUFFER_LEN: usize = FRAMEBUFFER_WIDTH * FRAMEBUFFER_HEIGHT;

const CGB_RGB555_WHITE: u16 = 0x7FFF;
const CGB_BG_ATTR_PALETTE_MASK: u8 = 0x07;
const CGB_BG_ATTR_VRAM_BANK: u8 = 0x08;
const CGB_BG_ATTR_X_FLIP: u8 = 0x20;
const CGB_BG_ATTR_Y_FLIP: u8 = 0x40;
const CGB_BG_ATTR_PRIORITY: u8 = 0x80;
const CGB_OBJ_ATTR_PALETTE_MASK: u8 = 0x07;
const CGB_OBJ_ATTR_VRAM_BANK: u8 = 0x08;

const INTERRUPT_VBLANK_BIT: u8 = 0x01;
const INTERRUPT_STAT_BIT: u8 = 0x02;
const INTERRUPT_ENABLE_VBLANK_BIT: u8 = 0x01;
const INTERRUPT_ENABLE_STAT_BIT: u8 = 0x02;

const BG_MAP_0_OFFSET: usize = 0x1800; // 0x9800-0x9BFF
const BG_MAP_1_OFFSET: usize = 0x1C00; // 0x9C00-0x9FFF
const TILE_BLOCK_0_OFFSET: usize = 0x0000; // 0x8000-0x87FF
const TILE_BLOCK_2_OFFSET: usize = 0x1000; // 0x9000-0x97FF

bitflags! {
    /// Typed view of LCDC (`FF40`) control bits.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    pub struct Lcdc: u8 {
        const BG_WINDOW_ENABLE = 0x01;
        const OBJ_ENABLE = 0x02;
        const OBJ_SIZE = 0x04;
        const BG_TILE_MAP_AREA = 0x08;
        const BG_WINDOW_TILE_DATA_AREA = 0x10;
        const WINDOW_ENABLE = 0x20;
        const WINDOW_TILE_MAP_AREA = 0x40;
        const LCD_ENABLE = 0x80;
    }
}

impl Lcdc {
    pub const fn read_bits(self) -> u8 {
        self.bits()
    }

    pub fn write_bits(&mut self, value: u8) {
        *self = Self::from_bits_retain(value);
    }

    const fn enabled(self) -> bool {
        self.contains(Self::LCD_ENABLE)
    }

    const fn bg_window_enabled(self) -> bool {
        self.contains(Self::BG_WINDOW_ENABLE)
    }

    const fn obj_enabled(self) -> bool {
        self.contains(Self::OBJ_ENABLE)
    }

    const fn tall_objs(self) -> bool {
        self.contains(Self::OBJ_SIZE)
    }

    const fn window_enabled(self) -> bool {
        self.contains(Self::WINDOW_ENABLE)
    }

    const fn window_map_base_offset(self) -> usize {
        if self.contains(Self::WINDOW_TILE_MAP_AREA) {
            BG_MAP_1_OFFSET
        } else {
            BG_MAP_0_OFFSET
        }
    }

    const fn bg_map_base_offset(self) -> usize {
        if self.contains(Self::BG_TILE_MAP_AREA) {
            BG_MAP_1_OFFSET
        } else {
            BG_MAP_0_OFFSET
        }
    }

    const fn unsigned_tile_data(self) -> bool {
        self.contains(Self::BG_WINDOW_TILE_DATA_AREA)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum PpuMode {
    HBlank = 0,
    VBlank = 1,
    OamScan = 2,
    Drawing = 3,
}

impl PpuMode {
    const fn from_bits(bits: u8) -> Self {
        match bits & 0x03 {
            0 => Self::HBlank,
            1 => Self::VBlank,
            2 => Self::OamScan,
            _ => Self::Drawing,
        }
    }

    const fn bits(self) -> u8 {
        self as u8
    }
}

bitflags! {
    /// Writable STAT (`FF41`) interrupt-source enable bits.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    pub struct StatInterruptSources: u8 {
        const HBLANK = 0x08;
        const VBLANK = 0x10;
        const OAM = 0x20;
        const LYC_EQUAL = 0x40;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Stat {
    bits: u8,
}

impl Default for Stat {
    fn default() -> Self {
        Self { bits: 0x80 }
    }
}

impl From<u8> for Stat {
    fn from(value: u8) -> Self {
        Self { bits: value }
    }
}

impl From<Stat> for u8 {
    fn from(value: Stat) -> Self {
        value.read_bits()
    }
}

impl Stat {
    const LYC_EQUAL_BIT: u8 = 0x04;
    const READ_ONLY_MASK: u8 = 0x07;
    const WRITABLE_MASK: u8 = 0x78;
    const READ_RESERVED: u8 = 0x80;

    pub const fn read_bits(self) -> u8 {
        self.bits | Self::READ_RESERVED
    }

    pub fn write_bits_preserving_status(&mut self, value: u8) {
        let readonly_bits = self.bits & Self::READ_ONLY_MASK;
        self.bits = Self::READ_RESERVED | readonly_bits | (value & Self::WRITABLE_MASK);
    }

    const fn mode(self) -> PpuMode {
        PpuMode::from_bits(self.bits)
    }

    fn set_mode(&mut self, mode: PpuMode) {
        self.bits = (self.bits & !0x03) | mode.bits();
    }

    const fn lyc_equal(self) -> bool {
        (self.bits & Self::LYC_EQUAL_BIT) != 0
    }

    fn set_lyc_equal(&mut self, enabled: bool) {
        if enabled {
            self.bits |= Self::LYC_EQUAL_BIT;
        } else {
            self.bits &= !Self::LYC_EQUAL_BIT;
        }
    }

    const fn interrupt_sources(self) -> StatInterruptSources {
        StatInterruptSources::from_bits_truncate(self.bits)
    }

    const fn has_interrupt_source(self, source: StatInterruptSources) -> bool {
        self.interrupt_sources().contains(source)
    }
}

bitflags! {
    /// Typed view of one OAM sprite's attribute byte.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    pub struct SpriteAttributes: u8 {
        const DMG_PALETTE_1 = 0x10;
        const X_FLIP = 0x20;
        const Y_FLIP = 0x40;
        const PRIORITY = 0x80;
    }
}

impl SpriteAttributes {
    pub const fn read_bits(self) -> u8 {
        self.bits()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct PaletteRegister(u8);

impl From<u8> for PaletteRegister {
    fn from(value: u8) -> Self {
        Self(value)
    }
}

impl From<PaletteRegister> for u8 {
    fn from(value: PaletteRegister) -> Self {
        value.read_bits()
    }
}

impl PaletteRegister {
    pub const fn read_bits(self) -> u8 {
        self.0
    }

    pub fn write_bits(&mut self, value: u8) {
        self.0 = value;
    }

    pub const fn shade(self, color_id: u8) -> u8 {
        let shift = (color_id & 0x03) * 2;
        (self.0 >> shift) & 0x03
    }
}

#[cfg(test)]
const LCDC_ENABLE: u8 = Lcdc::LCD_ENABLE.bits();
#[cfg(test)]
const LCDC_BG_ENABLE: u8 = Lcdc::BG_WINDOW_ENABLE.bits();
#[cfg(test)]
const LCDC_SPRITE_ENABLE: u8 = Lcdc::OBJ_ENABLE.bits();
#[cfg(test)]
const LCDC_SPRITE_SIZE: u8 = Lcdc::OBJ_SIZE.bits();
#[cfg(test)]
const LCDC_WINDOW_ENABLE: u8 = Lcdc::WINDOW_ENABLE.bits();
#[cfg(test)]
const LCDC_WINDOW_TILE_MAP_SELECT: u8 = Lcdc::WINDOW_TILE_MAP_AREA.bits();
#[cfg(test)]
const LCDC_BG_TILE_MAP_SELECT: u8 = Lcdc::BG_TILE_MAP_AREA.bits();
#[cfg(test)]
const LCDC_BG_TILE_DATA_SELECT: u8 = Lcdc::BG_WINDOW_TILE_DATA_AREA.bits();
#[cfg(test)]
const STAT_COINCIDENCE_INTERRUPT: u8 = StatInterruptSources::LYC_EQUAL.bits();
#[cfg(test)]
const STAT_MODE_2_INTERRUPT: u8 = StatInterruptSources::OAM.bits();
#[cfg(test)]
const STAT_MODE_1_INTERRUPT: u8 = StatInterruptSources::VBLANK.bits();
#[cfg(test)]
const STAT_MODE_0_INTERRUPT: u8 = StatInterruptSources::HBLANK.bits();
#[cfg(test)]
const STAT_LYC_EQUAL: u8 = Stat::LYC_EQUAL_BIT;
#[cfg(test)]
const STAT_MODE: u8 = 0x03;
#[cfg(test)]
const SPRITE_ATTRIBUTE_PRIORITY: u8 = SpriteAttributes::PRIORITY.bits();
#[cfg(test)]
const SPRITE_ATTRIBUTE_Y_FLIP: u8 = SpriteAttributes::Y_FLIP.bits();
#[cfg(test)]
const SPRITE_ATTRIBUTE_X_FLIP: u8 = SpriteAttributes::X_FLIP.bits();
#[cfg(test)]
const SPRITE_ATTRIBUTE_PALETTE: u8 = SpriteAttributes::DMG_PALETTE_1.bits();

/// Resolves a 2-bit DMG palette shade (0-3) from a palette register and logical color id.
///
/// DMG palette registers (`BGP`, `OBP0`, `OBP1`) encode four 2-bit shade selectors:
/// - bits 1:0 map color id 0
/// - bits 3:2 map color id 1
/// - bits 5:4 map color id 2
/// - bits 7:6 map color id 3
pub fn dmg_palette_shade(palette: impl Into<PaletteRegister>, color_id: u8) -> u8 {
    palette.into().shade(color_id)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpritePixel {
    pub color_id: u8,
    pub use_obp1: bool,
    pub cgb_palette: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BackgroundPixel {
    color_id: u8,
    cgb_palette: u8,
    cgb_priority: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Ppu {
    vram: [u8; VRAM_SIZE],
    selected_vram_bank: u8,
    cgb_mode_enabled: bool,
    bg_palette_index: u8,
    obj_palette_index: u8,
    cgb_bg_palette_ram: [u8; CGB_PALETTE_RAM_SIZE],
    cgb_obj_palette_ram: [u8; CGB_PALETTE_RAM_SIZE],
    oam: [u8; OAM_SIZE],
    lcdc: Lcdc,
    stat: Stat,
    scy: u8,
    scx: u8,
    ly: u8,
    lyc: u8,
    dma: u8,
    bgp: PaletteRegister,
    obp0: PaletteRegister,
    obp1: PaletteRegister,
    wy: u8,
    wx: u8,
    scanline_dot: u16,
    lcd_enable_delay_dots: u8,
    stat_irq_line_high: bool,
    stat_irq_pending: bool,
    /// Most recently rendered DMG-compatible shade frame in row-major order (`y * 160 + x`).
    ///
    /// Pixel format is a DMG shade index per byte:
    /// - `0` = white
    /// - `1` = light gray
    /// - `2` = dark gray
    /// - `3` = black
    ///
    /// The PPU owns this storage for the lifetime of the `Ppu`. Consumers can borrow
    /// read-only views through [`Self::framebuffer`] / [`Self::framebuffer_pixels`] and
    /// should snapshot/copy if they need to retain frame data beyond a mutable emulator tick.
    framebuffer: [u8; FRAMEBUFFER_LEN],
    /// Most recently rendered color frame in row-major order using RGB555 pixels.
    ///
    /// DMG mode mirrors the effective grayscale DMG palette into RGB555; CGB mode
    /// resolves BG/window and OBJ color data through CGB palette RAM.
    color_framebuffer: [u16; FRAMEBUFFER_LEN],
    frame_ready_pending: bool,
}

impl Default for Ppu {
    fn default() -> Self {
        Self {
            vram: [0; VRAM_SIZE],
            selected_vram_bank: 0,
            cgb_mode_enabled: false,
            bg_palette_index: 0,
            obj_palette_index: 0,
            cgb_bg_palette_ram: [0; CGB_PALETTE_RAM_SIZE],
            cgb_obj_palette_ram: [0; CGB_PALETTE_RAM_SIZE],
            oam: [0; OAM_SIZE],
            lcdc: Lcdc::empty(),
            stat: Stat::default(),
            scy: 0,
            scx: 0,
            ly: 0,
            lyc: 0,
            dma: 0,
            bgp: PaletteRegister::default(),
            obp0: PaletteRegister::default(),
            obp1: PaletteRegister::default(),
            wy: 0,
            wx: 0,
            scanline_dot: 0,
            lcd_enable_delay_dots: 0,
            stat_irq_line_high: false,
            stat_irq_pending: false,
            framebuffer: [0; FRAMEBUFFER_LEN],
            color_framebuffer: [CGB_RGB555_WHITE; FRAMEBUFFER_LEN],
            frame_ready_pending: false,
        }
    }
}

impl Ppu {
    pub fn new_cgb() -> Self {
        Self {
            cgb_mode_enabled: true,
            ..Self::default()
        }
    }

    pub const fn cgb_mode_enabled(&self) -> bool {
        self.cgb_mode_enabled
    }

    pub fn set_cgb_mode_enabled(&mut self, enabled: bool) {
        self.cgb_mode_enabled = enabled;
        if !enabled {
            self.selected_vram_bank = 0;
        }
    }

    pub const fn selected_vram_bank(&self) -> u8 {
        self.selected_vram_bank
    }

    pub const fn scanline_dot(&self) -> u16 {
        self.scanline_dot
    }

    pub const fn lcd_enable_delay_dots(&self) -> u8 {
        self.lcd_enable_delay_dots
    }

    fn window_map_base_offset(&self) -> usize {
        self.lcdc.window_map_base_offset()
    }

    fn bg_map_base_offset(&self) -> usize {
        self.lcdc.bg_map_base_offset()
    }

    fn tile_data_row_offset(&self, tile_index: u8, row_in_tile: u8) -> usize {
        let row_base = usize::from(row_in_tile) * 2;
        if self.lcdc.unsigned_tile_data() {
            TILE_BLOCK_0_OFFSET + usize::from(tile_index) * 16 + row_base
        } else {
            let signed_index = i8::from_ne_bytes([tile_index]);
            let tile_offset = isize::from(signed_index) * 16;
            (TILE_BLOCK_2_OFFSET as isize + tile_offset + row_base as isize) as usize
        }
    }

    fn current_mode(&self) -> u8 {
        self.stat.mode().bits()
    }

    fn set_mode(&mut self, mode: u8) {
        self.stat.set_mode(PpuMode::from_bits(mode));
    }

    fn update_lyc_coincidence_flag(&mut self) {
        if self.ly == self.lyc {
            self.stat.set_lyc_equal(true);
        } else {
            self.stat.set_lyc_equal(false);
        }
    }

    fn stat_irq_condition_active(&self) -> bool {
        let coincidence_enabled_and_true = self
            .stat
            .has_interrupt_source(StatInterruptSources::LYC_EQUAL)
            && self.stat.lyc_equal();

        if !self.lcdc.enabled() {
            return coincidence_enabled_and_true;
        }

        let mode = self.current_mode();
        let mode_enabled = (mode == 0
            && self.stat.has_interrupt_source(StatInterruptSources::HBLANK))
            || (mode == 1 && self.stat.has_interrupt_source(StatInterruptSources::VBLANK))
            || (mode == 2 && self.stat.has_interrupt_source(StatInterruptSources::OAM));

        mode_enabled || coincidence_enabled_and_true
    }

    fn request_stat_irq_edge(&mut self, interrupt_flag: Option<&mut u8>) {
        if let Some(flag) = interrupt_flag {
            *flag |= INTERRUPT_STAT_BIT;
        } else {
            self.stat_irq_pending = true;
        }
    }

    fn update_stat_irq_line(&mut self, interrupt_flag: Option<&mut u8>) {
        let next_line_high = self.stat_irq_condition_active();
        if !self.stat_irq_line_high && next_line_high {
            self.request_stat_irq_edge(interrupt_flag);
        }
        self.stat_irq_line_high = next_line_high;
    }

    fn stat_write_glitch_condition_active(&self) -> bool {
        if !self.lcdc.enabled() {
            return false;
        }

        self.current_mode() != 3 || self.stat.lyc_equal()
    }

    fn apply_stat_write_glitch(&mut self) {
        if !self.stat_irq_line_high && self.stat_write_glitch_condition_active() {
            self.request_stat_irq_edge(None);
        }
    }

    fn vram_accessible(&self) -> bool {
        self.current_mode() != 0x03
    }

    fn oam_accessible(&self) -> bool {
        !matches!(self.current_mode(), 0x02 | 0x03)
    }

    fn cgb_palette_data_accessible(&self) -> bool {
        self.current_mode() != 0x03
    }

    pub fn may_request_interrupt(&self, interrupt_enable: u8) -> bool {
        if !self.lcdc.enabled() {
            return false;
        }

        if (interrupt_enable & INTERRUPT_ENABLE_VBLANK_BIT) != 0 {
            return true;
        }

        if (interrupt_enable & INTERRUPT_ENABLE_STAT_BIT) == 0 {
            return false;
        }

        if self.stat.interrupt_sources().intersects(
            StatInterruptSources::HBLANK | StatInterruptSources::VBLANK | StatInterruptSources::OAM,
        ) {
            return true;
        }

        self.stat
            .has_interrupt_source(StatInterruptSources::LYC_EQUAL)
            && self.lyc < TOTAL_SCANLINES
    }

    pub fn read_vram(&self, address: u16) -> u8 {
        if !self.vram_accessible() {
            return 0xFF;
        }

        let offset = self.selected_vram_offset(address);
        self.vram[offset]
    }

    pub fn dma_read_vram(&self, address: u16) -> u8 {
        let offset = self.selected_vram_offset(address);
        self.vram[offset]
    }

    pub fn write_vram(&mut self, address: u16, value: u8) {
        if !self.vram_accessible() {
            return;
        }

        let offset = self.selected_vram_offset(address);
        self.vram[offset] = value;
    }

    pub fn read_oam(&self, address: u16) -> u8 {
        if !self.oam_accessible() {
            return 0xFF;
        }

        self.oam[(address - OAM_START) as usize]
    }

    pub fn dma_read_oam(&self, address: u16) -> u8 {
        self.oam[(address - OAM_START) as usize]
    }

    pub fn write_oam(&mut self, address: u16, value: u8) {
        if !self.oam_accessible() {
            return;
        }

        self.oam[(address - OAM_START) as usize] = value;
    }

    pub fn dma_write_oam(&mut self, offset: u8, value: u8) {
        self.oam[offset as usize] = value;
    }

    fn selected_vram_offset(&self, address: u16) -> usize {
        usize::from(self.selected_vram_bank) * VRAM_BANK_SIZE + (address - VRAM_START) as usize
    }

    fn read_cgb_palette_index(index: u8) -> u8 {
        CGB_PALETTE_INDEX_READ_MASK
            | (index & (CGB_PALETTE_AUTO_INCREMENT | CGB_PALETTE_INDEX_MASK))
    }

    fn increment_cgb_palette_index(index: &mut u8) {
        if (*index & CGB_PALETTE_AUTO_INCREMENT) != 0 {
            *index =
                CGB_PALETTE_AUTO_INCREMENT | ((*index).wrapping_add(1) & CGB_PALETTE_INDEX_MASK);
        }
    }

    pub fn read_register(&self, address: u16) -> Option<u8> {
        let value = match address {
            LCDC_REGISTER => self.lcdc.read_bits(),
            STAT_REGISTER => self.stat.read_bits(),
            SCY_REGISTER => self.scy,
            SCX_REGISTER => self.scx,
            LY_REGISTER => self.ly,
            LYC_REGISTER => self.lyc,
            DMA_REGISTER => self.dma,
            BGP_REGISTER => self.bgp.read_bits(),
            OBP0_REGISTER => self.obp0.read_bits(),
            OBP1_REGISTER => self.obp1.read_bits(),
            WY_REGISTER => self.wy,
            WX_REGISTER => self.wx,
            VBK_REGISTER => {
                if self.cgb_mode_enabled {
                    CGB_VBK_READ_MASK | self.selected_vram_bank
                } else {
                    0xFF
                }
            }
            BCPS_REGISTER => {
                if self.cgb_mode_enabled {
                    Self::read_cgb_palette_index(self.bg_palette_index)
                } else {
                    0xFF
                }
            }
            BCPD_REGISTER => {
                if self.cgb_mode_enabled && self.cgb_palette_data_accessible() {
                    self.cgb_bg_palette_ram
                        [(self.bg_palette_index & CGB_PALETTE_INDEX_MASK) as usize]
                } else {
                    0xFF
                }
            }
            OCPS_REGISTER => {
                if self.cgb_mode_enabled {
                    Self::read_cgb_palette_index(self.obj_palette_index)
                } else {
                    0xFF
                }
            }
            OCPD_REGISTER => {
                if self.cgb_mode_enabled && self.cgb_palette_data_accessible() {
                    self.cgb_obj_palette_ram
                        [(self.obj_palette_index & CGB_PALETTE_INDEX_MASK) as usize]
                } else {
                    0xFF
                }
            }
            _ => return None,
        };

        Some(value)
    }

    pub fn write_register(&mut self, address: u16, value: u8) -> bool {
        match address {
            LCDC_REGISTER => {
                let was_enabled = self.lcdc.enabled();
                self.lcdc.write_bits(value);
                let now_enabled = self.lcdc.enabled();

                if !was_enabled && now_enabled {
                    self.scanline_dot = 0;
                    self.ly = 0;
                    self.set_mode(0x00);
                    self.lcd_enable_delay_dots = LCD_ENABLE_STARTUP_DELAY_DOTS;
                    self.update_lyc_coincidence_flag();
                } else if was_enabled && !now_enabled {
                    self.scanline_dot = 0;
                    self.ly = 0;
                    self.set_mode(0x00);
                    self.lcd_enable_delay_dots = 0;
                    self.clear_framebuffer();
                    self.frame_ready_pending = false;
                }

                self.update_stat_irq_line(None);
            }
            STAT_REGISTER => {
                // DMG compatibility quirk: a write to STAT briefly behaves like all
                // STAT interrupt source enable bits are set. If the LCD is enabled
                // and the PPU is currently in an interruptable STAT condition, that
                // transient level can create a STAT IRQ edge even when the stored
                // value disables the source again.
                self.apply_stat_write_glitch();

                self.stat.write_bits_preserving_status(value);
                self.update_stat_irq_line(None);
            }
            SCY_REGISTER => self.scy = value,
            SCX_REGISTER => self.scx = value,
            LY_REGISTER => {
                self.ly = 0;
                if self.lcdc.enabled() {
                    self.update_lyc_coincidence_flag();
                    self.update_stat_irq_line(None);
                }
            }
            LYC_REGISTER => {
                self.lyc = value;
                if self.lcdc.enabled() {
                    self.update_lyc_coincidence_flag();
                    self.update_stat_irq_line(None);
                }
            }
            DMA_REGISTER => self.dma = value,
            BGP_REGISTER => self.bgp.write_bits(value),
            OBP0_REGISTER => self.obp0.write_bits(value),
            OBP1_REGISTER => self.obp1.write_bits(value),
            WY_REGISTER => self.wy = value,
            WX_REGISTER => self.wx = value,
            VBK_REGISTER => {
                if self.cgb_mode_enabled {
                    self.selected_vram_bank = value & 0x01;
                }
            }
            BCPS_REGISTER => {
                if self.cgb_mode_enabled {
                    self.bg_palette_index =
                        value & (CGB_PALETTE_AUTO_INCREMENT | CGB_PALETTE_INDEX_MASK);
                }
            }
            BCPD_REGISTER => {
                if self.cgb_mode_enabled && self.cgb_palette_data_accessible() {
                    self.cgb_bg_palette_ram
                        [(self.bg_palette_index & CGB_PALETTE_INDEX_MASK) as usize] = value;
                    Self::increment_cgb_palette_index(&mut self.bg_palette_index);
                }
            }
            OCPS_REGISTER => {
                if self.cgb_mode_enabled {
                    self.obj_palette_index =
                        value & (CGB_PALETTE_AUTO_INCREMENT | CGB_PALETTE_INDEX_MASK);
                }
            }
            OCPD_REGISTER => {
                if self.cgb_mode_enabled && self.cgb_palette_data_accessible() {
                    self.cgb_obj_palette_ram
                        [(self.obj_palette_index & CGB_PALETTE_INDEX_MASK) as usize] = value;
                    Self::increment_cgb_palette_index(&mut self.obj_palette_index);
                }
            }
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
    /// - Applies window positioning using `WX/WY` with the DMG `WX-7` rule when enabled.
    pub fn background_pixel_color_id(&self, screen_x: u8, screen_y: u8) -> u8 {
        self.background_pixel(screen_x, screen_y).color_id
    }

    fn background_pixel(&self, screen_x: u8, screen_y: u8) -> BackgroundPixel {
        if !self.cgb_mode_enabled && !self.lcdc.bg_window_enabled() {
            return BackgroundPixel {
                color_id: 0,
                cgb_palette: 0,
                cgb_priority: false,
            };
        }

        let window_visible = self.lcdc.window_enabled()
            && u16::from(screen_y) >= u16::from(self.wy)
            && u16::from(self.wx) <= 166
            && (u16::from(screen_x) + 7) >= u16::from(self.wx);

        let (map_base, fetch_x, fetch_y) = if window_visible {
            let window_x = (u16::from(screen_x) + 7 - u16::from(self.wx)) as u8;
            let window_y = screen_y.wrapping_sub(self.wy);
            (self.window_map_base_offset(), window_x, window_y)
        } else {
            (
                self.bg_map_base_offset(),
                screen_x.wrapping_add(self.scx),
                screen_y.wrapping_add(self.scy),
            )
        };

        let tile_col = (fetch_x / 8) as usize;
        let tile_row = (fetch_y / 8) as usize;
        let mut row_in_tile = fetch_y % 8;
        let mut pixel_in_tile = fetch_x % 8;

        let map_index = tile_row * 32 + tile_col;
        let tile_index = self.vram[map_base + map_index];
        let attributes = if self.cgb_mode_enabled {
            self.vram[VRAM_BANK_SIZE + map_base + map_index]
        } else {
            0
        };

        if (attributes & CGB_BG_ATTR_Y_FLIP) != 0 {
            row_in_tile = 7 - row_in_tile;
        }
        if (attributes & CGB_BG_ATTR_X_FLIP) != 0 {
            pixel_in_tile = 7 - pixel_in_tile;
        }

        let tile_bank_offset = if self.cgb_mode_enabled && (attributes & CGB_BG_ATTR_VRAM_BANK) != 0
        {
            VRAM_BANK_SIZE
        } else {
            0
        };
        let tile_row_offset = tile_bank_offset + self.tile_data_row_offset(tile_index, row_in_tile);
        let low = self.vram[tile_row_offset];
        let high = self.vram[tile_row_offset + 1];
        let bit = 7 - pixel_in_tile;

        let low_bit = (low >> bit) & 0x01;
        let high_bit = (high >> bit) & 0x01;
        BackgroundPixel {
            color_id: (high_bit << 1) | low_bit,
            cgb_palette: attributes & CGB_BG_ATTR_PALETTE_MASK,
            cgb_priority: (attributes & CGB_BG_ATTR_PRIORITY) != 0,
        }
    }

    /// Returns the visible sprite pixel at the given screen coordinate, if any.
    ///
    /// DMG selection rules covered here:
    /// - Sprite coordinates use `OAM.x - 8`, `OAM.y - 16` offsets.
    /// - Supports per-sprite X/Y flip and OBP0/OBP1 palette selection.
    /// - Honors sprite priority bit: when set, non-zero background pixels win.
    /// - Resolves overlapping sprites by DMG priority (lowest X, then lowest OAM index).
    /// - Supports both 8x8 and 8x16 object modes (LCDC bit 2).
    pub fn sprite_pixel(&self, screen_x: u8, screen_y: u8, bg_color_id: u8) -> Option<SpritePixel> {
        if !self.lcdc.obj_enabled() {
            return None;
        }

        let sprite_height = if self.lcdc.tall_objs() { 16 } else { 8 };

        let mut scanline_sprites = [0usize; MAX_SPRITES_PER_SCANLINE];
        let mut scanline_sprite_count = 0usize;
        let py = i16::from(screen_y);

        for sprite_index in 0..OAM_SPRITE_COUNT {
            let base = sprite_index * OAM_ENTRY_SIZE;
            let sprite_y = self.oam[base];
            let sprite_top = i16::from(sprite_y) - 16;
            if py >= sprite_top && py < sprite_top + sprite_height {
                scanline_sprites[scanline_sprite_count] = sprite_index;
                scanline_sprite_count += 1;
                if scanline_sprite_count == MAX_SPRITES_PER_SCANLINE {
                    break;
                }
            }
        }

        let mut candidate: Option<(u8, usize, SpritePixel, SpriteAttributes)> = None;
        for sprite_index in scanline_sprites.into_iter().take(scanline_sprite_count) {
            let base = sprite_index * OAM_ENTRY_SIZE;
            let sprite_y = self.oam[base];
            let sprite_x = self.oam[base + 1];
            let tile_index = self.oam[base + 2];
            let attributes = SpriteAttributes::from_bits_retain(self.oam[base + 3]);

            let sprite_top = i16::from(sprite_y) - 16;
            let sprite_left = i16::from(sprite_x) - 8;
            let px = i16::from(screen_x);

            if px < sprite_left
                || px >= sprite_left + 8
                || py < sprite_top
                || py >= sprite_top + sprite_height
            {
                continue;
            }

            let mut row = (py - sprite_top) as u8;
            let mut col = (px - sprite_left) as u8;
            if attributes.contains(SpriteAttributes::Y_FLIP) {
                row = (sprite_height - 1) as u8 - row;
            }
            if attributes.contains(SpriteAttributes::X_FLIP) {
                col = 7 - col;
            }

            let tile_id = if sprite_height == 16 {
                let top_tile = tile_index & 0xFE;
                top_tile.wrapping_add(row / 8)
            } else {
                tile_index
            };
            let row_in_tile = row & 0x07;

            let tile_bank_offset =
                if self.cgb_mode_enabled && (attributes.bits() & CGB_OBJ_ATTR_VRAM_BANK) != 0 {
                    VRAM_BANK_SIZE
                } else {
                    0
                };
            let tile_row_offset = tile_bank_offset
                + TILE_BLOCK_0_OFFSET
                + usize::from(tile_id) * 16
                + usize::from(row_in_tile) * 2;
            let low = self.vram[tile_row_offset];
            let high = self.vram[tile_row_offset + 1];
            let bit = 7 - col;
            let color_id = (((high >> bit) & 1) << 1) | ((low >> bit) & 1);
            if color_id == 0 {
                continue;
            }

            let pixel = SpritePixel {
                color_id,
                use_obp1: attributes.contains(SpriteAttributes::DMG_PALETTE_1),
                cgb_palette: attributes.bits() & CGB_OBJ_ATTR_PALETTE_MASK,
            };

            match candidate {
                None => candidate = Some((sprite_x, sprite_index, pixel, attributes)),
                Some((best_x, best_index, _, _))
                    if (!self.cgb_mode_enabled
                        && (sprite_x < best_x
                            || (sprite_x == best_x && sprite_index < best_index)))
                        || (self.cgb_mode_enabled && sprite_index < best_index) =>
                {
                    candidate = Some((sprite_x, sprite_index, pixel, attributes));
                }
                _ => {}
            }
        }

        candidate.and_then(|(_, _, pixel, attributes)| {
            let bg_can_mask_obj = self.lcdc.bg_window_enabled() && bg_color_id != 0;
            if attributes.contains(SpriteAttributes::PRIORITY) && bg_can_mask_obj {
                None
            } else {
                Some(pixel)
            }
        })
    }

    /// Returns the final DMG shade index (0-3) for the background/window layer.
    pub fn background_pixel_shade(&self, screen_x: u8, screen_y: u8) -> u8 {
        if !self.lcdc.enabled() {
            return 0;
        }
        if !self.cgb_mode_enabled && !self.lcdc.bg_window_enabled() {
            return 0;
        }

        let color_id = self.background_pixel_color_id(screen_x, screen_y);
        self.bgp.shade(color_id)
    }

    /// Returns the final DMG shade index (0-3) for the composited pixel at `(x, y)`.
    ///
    /// Sprite priority and transparency are resolved via [`Self::sprite_pixel`], then the
    /// selected BGP/OBP palette register is applied to obtain the framebuffer shade.
    pub fn composited_pixel_shade(&self, screen_x: u8, screen_y: u8) -> u8 {
        if !self.lcdc.enabled() {
            return 0;
        }

        let bg = self.background_pixel(screen_x, screen_y);
        if let Some(sprite) = self.sprite_pixel(screen_x, screen_y, bg.color_id) {
            let palette = if sprite.use_obp1 {
                self.obp1
            } else {
                self.obp0
            };
            palette.shade(sprite.color_id)
        } else if !self.cgb_mode_enabled && !self.lcdc.bg_window_enabled() {
            0
        } else {
            self.bgp.shade(bg.color_id)
        }
    }

    /// Returns the final RGB555 color for the composited pixel at `(x, y)`.
    pub fn composited_pixel_rgb555(&self, screen_x: u8, screen_y: u8) -> u16 {
        if !self.lcdc.enabled() {
            return CGB_RGB555_WHITE;
        }

        let bg = self.background_pixel(screen_x, screen_y);
        if let Some(sprite) = self.sprite_pixel(screen_x, screen_y, bg.color_id) {
            if self.cgb_mode_enabled
                && self.lcdc.bg_window_enabled()
                && bg.cgb_priority
                && bg.color_id != 0
            {
                self.background_pixel_rgb555(bg)
            } else {
                self.sprite_pixel_rgb555(sprite)
            }
        } else if !self.lcdc.bg_window_enabled() && !self.cgb_mode_enabled {
            Self::dmg_shade_rgb555(0)
        } else {
            self.background_pixel_rgb555(bg)
        }
    }

    fn cgb_palette_rgb555(
        palette_ram: &[u8; CGB_PALETTE_RAM_SIZE],
        palette: u8,
        color_id: u8,
    ) -> u16 {
        let offset = usize::from((palette & 0x07) * 8 + (color_id & 0x03) * 2);
        u16::from(palette_ram[offset]) | (u16::from(palette_ram[offset + 1] & 0x7F) << 8)
    }

    fn dmg_shade_rgb555(shade: u8) -> u16 {
        match shade & 0x03 {
            0 => 0x7FFF,
            1 => 0x56B5,
            2 => 0x2D6B,
            _ => 0x0000,
        }
    }

    fn background_pixel_rgb555(&self, bg: BackgroundPixel) -> u16 {
        if self.cgb_mode_enabled {
            Self::cgb_palette_rgb555(&self.cgb_bg_palette_ram, bg.cgb_palette, bg.color_id)
        } else if !self.lcdc.bg_window_enabled() {
            Self::dmg_shade_rgb555(0)
        } else {
            Self::dmg_shade_rgb555(self.bgp.shade(bg.color_id))
        }
    }

    fn sprite_pixel_rgb555(&self, sprite: SpritePixel) -> u16 {
        if self.cgb_mode_enabled {
            Self::cgb_palette_rgb555(
                &self.cgb_obj_palette_ram,
                sprite.cgb_palette,
                sprite.color_id,
            )
        } else {
            let palette = if sprite.use_obp1 {
                self.obp1
            } else {
                self.obp0
            };
            Self::dmg_shade_rgb555(palette.shade(sprite.color_id))
        }
    }

    pub fn take_stat_irq_pending(&mut self) -> bool {
        let pending = self.stat_irq_pending;
        self.stat_irq_pending = false;
        pending
    }

    pub fn take_frame_ready(&mut self) -> bool {
        let pending = self.frame_ready_pending;
        self.frame_ready_pending = false;
        pending
    }

    /// Returns the owned framebuffer as a fixed-size row-major array.
    ///
    /// Layout contract: `index = y * 160 + x`, where `x ∈ 0..160`, `y ∈ 0..144`.
    /// Each byte is a DMG shade index `0..=3`.
    pub const fn framebuffer(&self) -> &[u8; FRAMEBUFFER_LEN] {
        &self.framebuffer
    }

    /// Returns the owned framebuffer as a flat pixel slice.
    ///
    /// This is equivalent to [`Self::framebuffer`] but convenient for generic APIs
    /// that consume `&[u8]`.
    pub fn framebuffer_pixels(&self) -> &[u8] {
        &self.framebuffer
    }

    /// Returns the owned color framebuffer as a fixed-size row-major RGB555 array.
    pub const fn color_framebuffer(&self) -> &[u16; FRAMEBUFFER_LEN] {
        &self.color_framebuffer
    }

    /// Returns the owned color framebuffer as a flat RGB555 pixel slice.
    pub fn color_framebuffer_pixels(&self) -> &[u16] {
        &self.color_framebuffer
    }

    fn render_visible_scanline(&mut self, scanline: u8) {
        let row_base = usize::from(scanline) * FRAMEBUFFER_WIDTH;
        for x in 0..FRAMEBUFFER_WIDTH {
            self.framebuffer[row_base + x] = self.composited_pixel_shade(x as u8, scanline);
            self.color_framebuffer[row_base + x] = self.composited_pixel_rgb555(x as u8, scanline);
        }
    }

    fn clear_framebuffer(&mut self) {
        self.framebuffer = [0; FRAMEBUFFER_LEN];
        self.color_framebuffer = [CGB_RGB555_WHITE; FRAMEBUFFER_LEN];
    }

    pub fn step(&mut self, interrupt_flag: &mut u8) {
        if !self.lcdc.enabled() {
            self.scanline_dot = 0;
            self.ly = 0;
            self.lcd_enable_delay_dots = 0;
            self.set_mode(0);
            self.update_stat_irq_line(Some(interrupt_flag));
            return;
        }

        if self.lcd_enable_delay_dots != 0 {
            self.lcd_enable_delay_dots -= 1;
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
        } else if self.scanline_dot < (MODE0_CYCLES_END - 4) {
            3
        } else {
            0
        };

        if previous_mode == 3 && next_mode == 0 && self.ly < VISIBLE_SCANLINES {
            self.render_visible_scanline(self.ly);
        }

        if previous_mode != 1 && next_mode == 1 {
            *interrupt_flag |= INTERRUPT_VBLANK_BIT;
            self.frame_ready_pending = true;
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
    fn lcd_disable_preserves_latched_stat_coincidence_bit() {
        let mut ppu = Ppu::default();
        ppu.write_register(LCDC_REGISTER, LCDC_ENABLE);
        ppu.write_register(LYC_REGISTER, 0x01);

        // Force a latched coincidence state before LCD disable.
        ppu.stat.set_lyc_equal(true);
        ppu.write_register(LCDC_REGISTER, 0x00);

        let stat = ppu.read_register(STAT_REGISTER).unwrap();
        assert_eq!(stat & STAT_MODE, 0x00);
        assert_eq!(stat & STAT_LYC_EQUAL, STAT_LYC_EQUAL);

        // Writing STAT should still preserve read-only mode/coincidence bits.
        ppu.write_register(STAT_REGISTER, 0x00);
        let stat_after_write = ppu.read_register(STAT_REGISTER).unwrap();
        assert_eq!(stat_after_write & STAT_MODE, 0x00);
        assert_eq!(stat_after_write & STAT_LYC_EQUAL, STAT_LYC_EQUAL);
    }

    #[test]
    fn vram_access_is_blocked_during_mode_3() {
        let mut ppu = Ppu::default();

        ppu.write_vram(0x8000, 0x12);
        assert_eq!(ppu.read_vram(0x8000), 0x12);

        ppu.stat.set_mode(PpuMode::from_bits(0x03));
        assert_eq!(ppu.read_vram(0x8000), 0xFF);

        ppu.write_vram(0x8000, 0x34);
        ppu.stat.set_mode(PpuMode::HBlank);
        assert_eq!(ppu.read_vram(0x8000), 0x12);
    }

    #[test]
    fn oam_access_is_blocked_during_modes_2_and_3() {
        let mut ppu = Ppu::default();

        ppu.write_oam(0xFE00, 0x56);
        assert_eq!(ppu.read_oam(0xFE00), 0x56);

        ppu.stat.set_mode(PpuMode::from_bits(0x02));
        assert_eq!(ppu.read_oam(0xFE00), 0xFF);
        ppu.write_oam(0xFE00, 0x78);

        ppu.stat.set_mode(PpuMode::from_bits(0x03));
        assert_eq!(ppu.read_oam(0xFE00), 0xFF);

        ppu.stat.set_mode(PpuMode::HBlank);
        assert_eq!(ppu.read_oam(0xFE00), 0x56);
    }

    #[test]
    fn dma_write_oam_bypasses_mode_restrictions() {
        let mut ppu = Ppu::default();
        ppu.stat.set_mode(PpuMode::from_bits(0x03));

        ppu.dma_write_oam(0, 0xAB);

        ppu.stat.set_mode(PpuMode::HBlank);
        assert_eq!(ppu.read_oam(0xFE00), 0xAB);
    }

    #[test]
    fn dma_reads_bypass_vram_and_oam_mode_restrictions() {
        let mut ppu = Ppu::default();
        ppu.write_vram(0x8000, 0x11);
        ppu.write_oam(0xFE00, 0x22);

        ppu.stat.set_mode(PpuMode::from_bits(0x03));
        assert_eq!(ppu.read_vram(0x8000), 0xFF);
        assert_eq!(ppu.read_oam(0xFE00), 0xFF);

        assert_eq!(ppu.dma_read_vram(0x8000), 0x11);
        assert_eq!(ppu.dma_read_oam(0xFE00), 0x22);
    }

    #[test]
    fn sprite_pixel_uses_dma_written_oam_data_during_mode_3() {
        let mut ppu = Ppu::default();
        ppu.write_register(LCDC_REGISTER, LCDC_SPRITE_ENABLE);
        ppu.write_vram(0x8010, 0b1000_0000);
        ppu.write_vram(0x8011, 0x00);

        ppu.stat.set_mode(PpuMode::from_bits(0x03));
        ppu.write_oam(0xFE00, 16);
        assert_eq!(ppu.read_oam(0xFE00), 0xFF);

        ppu.dma_write_oam(0, 16);
        ppu.dma_write_oam(1, 8);
        ppu.dma_write_oam(2, 0x01);
        ppu.dma_write_oam(3, 0x00);

        assert_eq!(
            ppu.sprite_pixel(0, 0, 0),
            Some(SpritePixel {
                color_id: 1,
                use_obp1: false,
                cgb_palette: 0
            })
        );
    }

    #[test]
    fn step_transitions_through_scanline_modes() {
        let mut ppu = Ppu::default();
        let mut interrupt_flag = 0u8;
        ppu.write_register(LCDC_REGISTER, 0x80);

        for _ in 0..LCD_ENABLE_STARTUP_DELAY_DOTS {
            ppu.step(&mut interrupt_flag);
            assert_eq!(ppu.read_register(STAT_REGISTER).unwrap() & 0x03, 0x00);
        }

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
        ppu.write_register(STAT_REGISTER, STAT_MODE_1_INTERRUPT);

        for _ in 0..LCD_ENABLE_STARTUP_DELAY_DOTS {
            ppu.step(&mut interrupt_flag);
        }

        let cycles_to_vblank = (CYCLES_PER_SCANLINE as u32) * (VISIBLE_SCANLINES as u32);
        for _ in 0..cycles_to_vblank {
            ppu.step(&mut interrupt_flag);
        }

        assert_eq!(ppu.read_register(LY_REGISTER), Some(VISIBLE_SCANLINES));
        assert_eq!(ppu.read_register(STAT_REGISTER).unwrap() & 0x03, 0x01);
        assert_eq!(interrupt_flag & INTERRUPT_VBLANK_BIT, INTERRUPT_VBLANK_BIT);
        assert_eq!(interrupt_flag & INTERRUPT_STAT_BIT, INTERRUPT_STAT_BIT);
        assert!(ppu.take_frame_ready());
        assert!(!ppu.take_frame_ready());
    }

    #[test]
    fn framebuffer_has_expected_size_and_row_major_layout() {
        let ppu = Ppu::default();

        assert_eq!(ppu.framebuffer().len(), FRAMEBUFFER_LEN);
        assert_eq!(ppu.framebuffer_pixels().len(), FRAMEBUFFER_LEN);
        assert_eq!(FRAMEBUFFER_WIDTH, 160);
        assert_eq!(FRAMEBUFFER_HEIGHT, 144);
        assert_eq!(FRAMEBUFFER_LEN, 160 * 144);
        assert_eq!(
            FRAMEBUFFER_WIDTH * (FRAMEBUFFER_HEIGHT - 1) + (FRAMEBUFFER_WIDTH - 1),
            FRAMEBUFFER_LEN - 1
        );
    }

    #[test]
    fn framebuffer_updates_as_scanlines_complete() {
        let mut ppu = Ppu::default();
        let mut interrupt_flag = 0u8;
        ppu.write_register(
            LCDC_REGISTER,
            LCDC_ENABLE | LCDC_BG_ENABLE | LCDC_BG_TILE_DATA_SELECT,
        );
        ppu.write_register(BGP_REGISTER, 0xE4);

        ppu.write_vram(0x9800, 0x01);
        for row in 0..8u16 {
            ppu.write_vram(0x8010 + row * 2, 0xFF);
            ppu.write_vram(0x8011 + row * 2, 0x00);
            ppu.write_vram(0x8020 + row * 2, 0x00);
            ppu.write_vram(0x8021 + row * 2, 0xFF);
        }

        for _ in 0..LCD_ENABLE_STARTUP_DELAY_DOTS {
            ppu.step(&mut interrupt_flag);
        }

        for _ in 0..MODE0_CYCLES_END {
            ppu.step(&mut interrupt_flag);
        }
        assert_eq!(ppu.framebuffer()[0], 1);
        assert_eq!(ppu.framebuffer()[FRAMEBUFFER_WIDTH], 0);

        ppu.write_vram(0x9800, 0x02);
        for _ in 0..CYCLES_PER_SCANLINE {
            ppu.step(&mut interrupt_flag);
        }
        assert_eq!(ppu.framebuffer()[FRAMEBUFFER_WIDTH], 2);
    }

    #[test]
    fn frame_ready_is_single_pulse_per_frame_with_coherent_framebuffer() {
        let mut ppu = Ppu::default();
        let mut interrupt_flag = 0u8;
        ppu.write_register(
            LCDC_REGISTER,
            LCDC_ENABLE | LCDC_BG_ENABLE | LCDC_BG_TILE_DATA_SELECT,
        );
        ppu.write_register(BGP_REGISTER, 0xE4);

        ppu.write_vram(0x9800, 0x01);
        ppu.write_vram(0x9A20, 0x02);
        ppu.write_vram(0x8010, 0xFF);
        ppu.write_vram(0x8011, 0x00);
        ppu.write_vram(0x802e, 0x00);
        ppu.write_vram(0x802f, 0xFF);

        for _ in 0..LCD_ENABLE_STARTUP_DELAY_DOTS {
            ppu.step(&mut interrupt_flag);
        }

        let cycles_to_vblank = usize::from(CYCLES_PER_SCANLINE) * usize::from(VISIBLE_SCANLINES);
        for _ in 0..cycles_to_vblank {
            ppu.step(&mut interrupt_flag);
        }

        assert!(ppu.take_frame_ready());
        assert!(!ppu.take_frame_ready());
        assert_eq!(ppu.framebuffer()[0], 1);
        assert_eq!(
            ppu.framebuffer()[FRAMEBUFFER_WIDTH * (FRAMEBUFFER_HEIGHT - 1)],
            2
        );
    }

    #[test]
    fn disabling_lcd_clears_framebuffer_to_blank() {
        let mut ppu = Ppu::default();
        let mut interrupt_flag = 0u8;
        ppu.write_register(
            LCDC_REGISTER,
            LCDC_ENABLE | LCDC_BG_ENABLE | LCDC_BG_TILE_DATA_SELECT,
        );
        ppu.write_register(BGP_REGISTER, 0xE4);
        ppu.write_vram(0x9800, 0x01);
        ppu.write_vram(0x8010, 0xFF);
        ppu.write_vram(0x8011, 0x00);

        for _ in 0..LCD_ENABLE_STARTUP_DELAY_DOTS {
            ppu.step(&mut interrupt_flag);
        }

        for _ in 0..MODE0_CYCLES_END {
            ppu.step(&mut interrupt_flag);
        }
        assert_eq!(ppu.framebuffer()[0], 1);

        ppu.write_register(LCDC_REGISTER, 0x00);

        assert!(ppu.framebuffer().iter().all(|&pixel| pixel == 0));
    }

    #[test]
    fn disabling_lcd_discards_pending_frame_ready_pulse() {
        let mut ppu = Ppu::default();
        let mut interrupt_flag = 0u8;
        ppu.write_register(
            LCDC_REGISTER,
            LCDC_ENABLE | LCDC_BG_ENABLE | LCDC_BG_TILE_DATA_SELECT,
        );
        ppu.write_register(BGP_REGISTER, 0xE4);
        ppu.write_vram(0x9800, 0x01);
        ppu.write_vram(0x8010, 0xFF);
        ppu.write_vram(0x8011, 0x00);

        for _ in 0..LCD_ENABLE_STARTUP_DELAY_DOTS {
            ppu.step(&mut interrupt_flag);
        }

        let cycles_to_vblank = usize::from(CYCLES_PER_SCANLINE) * usize::from(VISIBLE_SCANLINES);
        for _ in 0..cycles_to_vblank {
            ppu.step(&mut interrupt_flag);
        }
        assert!(ppu.framebuffer()[0] != 0);
        assert!(ppu.frame_ready_pending);

        ppu.write_register(LCDC_REGISTER, 0x00);

        assert!(ppu.framebuffer().iter().all(|&pixel| pixel == 0));
        assert!(!ppu.take_frame_ready());
    }

    #[test]
    fn lyc_match_sets_stat_coincidence_and_requests_stat_interrupt() {
        let mut ppu = Ppu::default();
        let mut interrupt_flag = 0u8;
        ppu.write_register(LCDC_REGISTER, 0x80);
        ppu.write_register(LYC_REGISTER, 0x01);
        ppu.write_register(STAT_REGISTER, STAT_COINCIDENCE_INTERRUPT);

        for _ in 0..LCD_ENABLE_STARTUP_DELAY_DOTS {
            ppu.step(&mut interrupt_flag);
        }

        for _ in 0..CYCLES_PER_SCANLINE {
            ppu.step(&mut interrupt_flag);
        }

        assert_eq!(ppu.read_register(LY_REGISTER), Some(0x01));
        assert_eq!(
            ppu.read_register(STAT_REGISTER).unwrap() & STAT_LYC_EQUAL,
            STAT_LYC_EQUAL
        );
        assert_eq!(interrupt_flag & INTERRUPT_STAT_BIT, INTERRUPT_STAT_BIT);
    }

    #[test]
    fn lcd_disable_preserves_coincidence_bit_when_currently_matching() {
        let mut ppu = Ppu::default();
        ppu.write_register(LCDC_REGISTER, LCDC_ENABLE);
        ppu.write_register(LYC_REGISTER, 0x00);

        assert_eq!(
            ppu.read_register(STAT_REGISTER).unwrap() & STAT_LYC_EQUAL,
            STAT_LYC_EQUAL
        );

        ppu.write_register(LCDC_REGISTER, 0x00);

        let stat = ppu.read_register(STAT_REGISTER).unwrap();
        assert_eq!(stat & STAT_MODE, 0x00);
        assert_eq!(stat & STAT_LYC_EQUAL, STAT_LYC_EQUAL);
        assert_eq!(ppu.read_register(LY_REGISTER), Some(0x00));
    }

    #[test]
    fn lcd_off_step_keeps_latched_coincidence_and_does_not_raise_stat() {
        let mut ppu = Ppu::default();
        ppu.write_register(LCDC_REGISTER, LCDC_ENABLE);
        ppu.write_register(LYC_REGISTER, 0x00);
        ppu.write_register(STAT_REGISTER, STAT_COINCIDENCE_INTERRUPT);
        assert!(ppu.take_stat_irq_pending());

        ppu.write_register(LCDC_REGISTER, 0x00);
        let mut interrupt_flag = 0u8;

        for _ in 0..8 {
            ppu.step(&mut interrupt_flag);
            let stat = ppu.read_register(STAT_REGISTER).unwrap();
            assert_eq!(stat & STAT_MODE, 0x00);
            assert_eq!(stat & STAT_LYC_EQUAL, STAT_LYC_EQUAL);
            assert_eq!(ppu.read_register(LY_REGISTER), Some(0x00));
        }

        assert_eq!(interrupt_flag & INTERRUPT_STAT_BIT, 0);
    }

    #[test]
    fn lcd_off_lyc_write_preserves_latched_coincidence_bit() {
        let mut ppu = Ppu::default();
        ppu.write_register(LCDC_REGISTER, LCDC_ENABLE);
        ppu.stat.set_lyc_equal(true);
        ppu.write_register(LCDC_REGISTER, 0x00);

        ppu.write_register(LYC_REGISTER, 0x01);

        assert_eq!(
            ppu.read_register(STAT_REGISTER).unwrap() & STAT_LYC_EQUAL,
            STAT_LYC_EQUAL
        );
    }

    #[test]
    fn may_request_interrupt_reflects_lcdc_ie_and_stat_sources() {
        let mut ppu = Ppu::default();
        ppu.write_register(LCDC_REGISTER, 0x80);

        assert!(ppu.may_request_interrupt(INTERRUPT_ENABLE_VBLANK_BIT));
        assert!(!ppu.may_request_interrupt(0x00));

        ppu.write_register(STAT_REGISTER, STAT_MODE_2_INTERRUPT);
        assert!(ppu.may_request_interrupt(INTERRUPT_ENABLE_STAT_BIT));

        ppu.write_register(STAT_REGISTER, STAT_COINCIDENCE_INTERRUPT);
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
        for _ in 0..LCD_ENABLE_STARTUP_DELAY_DOTS {
            ppu.step(&mut interrupt_flag);
            assert_eq!(ppu.read_register(STAT_REGISTER).unwrap() & 0x03, 0x00);
        }
        ppu.step(&mut interrupt_flag);
        assert_eq!(ppu.read_register(STAT_REGISTER).unwrap() & 0x03, 0x02);
        assert!(!ppu.take_stat_irq_pending());

        ppu.write_register(STAT_REGISTER, STAT_MODE_2_INTERRUPT);
        assert!(ppu.take_stat_irq_pending());
    }

    #[test]
    fn stat_write_glitch_queues_irq_for_active_mode_even_when_source_is_disabled() {
        let mut ppu = Ppu::default();
        let mut interrupt_flag = 0u8;
        ppu.write_register(LCDC_REGISTER, LCDC_ENABLE);

        for _ in 0..LCD_ENABLE_STARTUP_DELAY_DOTS {
            ppu.step(&mut interrupt_flag);
        }
        ppu.step(&mut interrupt_flag);
        assert_eq!(ppu.read_register(STAT_REGISTER).unwrap() & STAT_MODE, 0x02);
        assert!(!ppu.take_stat_irq_pending());

        ppu.write_register(STAT_REGISTER, 0x00);

        assert!(ppu.take_stat_irq_pending());
        assert_eq!(ppu.read_register(STAT_REGISTER).unwrap() & 0x78, 0x00);
    }

    #[test]
    fn stat_write_glitch_does_not_retrigger_while_stat_line_is_already_high() {
        let mut ppu = Ppu::default();
        let mut interrupt_flag = 0u8;
        ppu.write_register(LCDC_REGISTER, LCDC_ENABLE);

        for _ in 0..LCD_ENABLE_STARTUP_DELAY_DOTS {
            ppu.step(&mut interrupt_flag);
        }
        ppu.step(&mut interrupt_flag);
        assert_eq!(ppu.read_register(STAT_REGISTER).unwrap() & STAT_MODE, 0x02);

        ppu.write_register(STAT_REGISTER, STAT_MODE_2_INTERRUPT);
        assert!(ppu.take_stat_irq_pending());

        ppu.write_register(STAT_REGISTER, STAT_MODE_2_INTERRUPT);

        assert!(!ppu.take_stat_irq_pending());
    }

    #[test]
    fn enabling_or_matching_coincidence_condition_queues_stat_interrupt() {
        let mut ppu = Ppu::default();
        ppu.write_register(LCDC_REGISTER, 0x80);
        ppu.write_register(LYC_REGISTER, 0x00);
        assert_eq!(
            ppu.read_register(STAT_REGISTER).unwrap() & STAT_LYC_EQUAL,
            STAT_LYC_EQUAL
        );

        ppu.write_register(STAT_REGISTER, STAT_COINCIDENCE_INTERRUPT);
        assert!(ppu.take_stat_irq_pending());

        let mut ppu = Ppu::default();
        ppu.write_register(LCDC_REGISTER, 0x80);
        ppu.write_register(LYC_REGISTER, 0x01);
        ppu.write_register(STAT_REGISTER, STAT_COINCIDENCE_INTERRUPT);
        // Writing STAT while LCD is enabled can request the DMG STAT write
        // glitch; clear that edge before verifying the later LYC match edge.
        ppu.take_stat_irq_pending();
        ppu.write_register(LYC_REGISTER, 0x00);
        assert!(ppu.take_stat_irq_pending());
    }

    #[test]
    fn lcd_enable_starts_in_mode_0_until_ppu_is_clocked() {
        let mut ppu = Ppu::default();
        ppu.write_register(STAT_REGISTER, STAT_MODE_0_INTERRUPT);
        assert!(!ppu.take_stat_irq_pending());

        ppu.write_register(LCDC_REGISTER, 0x80);

        assert_eq!(ppu.read_register(STAT_REGISTER).unwrap() & 0x03, 0x00);
        assert!(ppu.take_stat_irq_pending());

        let mut interrupt_flag = 0u8;
        for _ in 0..LCD_ENABLE_STARTUP_DELAY_DOTS {
            ppu.step(&mut interrupt_flag);
            assert_eq!(ppu.read_register(STAT_REGISTER).unwrap() & 0x03, 0x00);
        }
        ppu.step(&mut interrupt_flag);
        assert_eq!(ppu.read_register(STAT_REGISTER).unwrap() & 0x03, 0x02);
    }

    #[test]
    fn lcd_enable_mode_2_stat_interrupt_only_appears_after_step() {
        let mut ppu = Ppu::default();
        ppu.write_register(STAT_REGISTER, STAT_MODE_2_INTERRUPT);
        assert!(!ppu.take_stat_irq_pending());

        ppu.write_register(LCDC_REGISTER, 0x80);

        assert_eq!(ppu.read_register(STAT_REGISTER).unwrap() & 0x03, 0x00);
        assert!(!ppu.take_stat_irq_pending());

        let mut interrupt_flag = 0u8;
        for _ in 0..LCD_ENABLE_STARTUP_DELAY_DOTS {
            ppu.step(&mut interrupt_flag);
            assert_eq!(ppu.read_register(STAT_REGISTER).unwrap() & 0x03, 0x00);
            assert_eq!(interrupt_flag & INTERRUPT_STAT_BIT, 0);
        }
        ppu.step(&mut interrupt_flag);
        assert_eq!(ppu.read_register(STAT_REGISTER).unwrap() & 0x03, 0x02);
        assert_eq!(interrupt_flag & INTERRUPT_STAT_BIT, INTERRUPT_STAT_BIT);
    }

    #[test]
    fn lcd_enable_immediate_stat_read_keeps_mode_0_with_latched_coincidence() {
        let mut ppu = Ppu::default();
        ppu.write_register(LCDC_REGISTER, LCDC_ENABLE);
        ppu.write_register(STAT_REGISTER, STAT_COINCIDENCE_INTERRUPT);
        ppu.write_register(LYC_REGISTER, 0x00);
        assert!(ppu.take_stat_irq_pending());

        ppu.write_register(LCDC_REGISTER, 0x00);
        assert_eq!(
            ppu.read_register(STAT_REGISTER).unwrap() & STAT_LYC_EQUAL,
            STAT_LYC_EQUAL
        );

        ppu.write_register(LCDC_REGISTER, LCDC_ENABLE);
        let stat = ppu.read_register(STAT_REGISTER).unwrap();
        assert_eq!(stat & STAT_MODE, 0x00);
        assert_eq!(stat & STAT_LYC_EQUAL, STAT_LYC_EQUAL);
        assert_eq!(stat & 0xC0, 0xC0);
    }

    #[test]
    fn lcd_enable_round1_recomputes_coincidence_without_immediate_mode_2() {
        let mut ppu = Ppu::default();
        ppu.write_register(LCDC_REGISTER, LCDC_ENABLE);
        ppu.write_register(STAT_REGISTER, STAT_COINCIDENCE_INTERRUPT);
        ppu.write_register(LYC_REGISTER, 0x00);
        assert!(ppu.take_stat_irq_pending());

        ppu.write_register(LCDC_REGISTER, 0x00);
        assert_eq!(
            ppu.read_register(STAT_REGISTER).unwrap() & STAT_LYC_EQUAL,
            STAT_LYC_EQUAL
        );

        // While LCD is off, writes should not update coincidence.
        ppu.write_register(LYC_REGISTER, 0x01);
        assert_eq!(
            ppu.read_register(STAT_REGISTER).unwrap() & STAT_LYC_EQUAL,
            STAT_LYC_EQUAL
        );

        // Re-enabling must recompute coincidence against LY=0 immediately.
        ppu.write_register(LCDC_REGISTER, LCDC_ENABLE);
        let stat = ppu.read_register(STAT_REGISTER).unwrap();
        assert_eq!(stat & STAT_MODE, 0x00);
        assert_eq!(stat & STAT_LYC_EQUAL, 0x00);
        assert!(!ppu.take_stat_irq_pending());
    }

    #[test]
    fn lcd_toggle_preserves_latched_coincidence_until_reenable_recomputes_stat() {
        const STAT_STABLE_MASK: u8 = 0x80 | 0x78 | STAT_LYC_EQUAL | STAT_MODE;

        let mut ppu = Ppu::default();
        ppu.write_register(LCDC_REGISTER, LCDC_ENABLE);
        ppu.write_register(STAT_REGISTER, STAT_COINCIDENCE_INTERRUPT);
        ppu.write_register(LYC_REGISTER, 0x00);

        // 1) Reach LY==LYC with coincidence set.
        assert_eq!(
            ppu.read_register(STAT_REGISTER).unwrap() & STAT_STABLE_MASK,
            0xC4
        );

        // 2) Disable LCD, coincidence stays latched.
        ppu.write_register(LCDC_REGISTER, 0x00);
        assert_eq!(
            ppu.read_register(STAT_REGISTER).unwrap() & STAT_STABLE_MASK,
            0xC4
        );

        // 3) LYC writes while LCD is off must not change the latched coincidence state.
        ppu.write_register(LYC_REGISTER, 0x01);
        assert_eq!(
            ppu.read_register(STAT_REGISTER).unwrap() & STAT_STABLE_MASK,
            0xC4
        );

        // 4) Re-enabling LCD immediately recomputes coincidence (LY=0, LYC=1) while mode is still 0.
        ppu.write_register(LCDC_REGISTER, LCDC_ENABLE);
        assert_eq!(
            ppu.read_register(STAT_REGISTER).unwrap() & STAT_STABLE_MASK,
            0xC0
        );
    }

    #[test]
    fn stat_line_handoff_between_sources_does_not_create_spurious_edge() {
        let mut ppu = Ppu::default();
        let mut interrupt_flag = 0u8;
        ppu.write_register(LCDC_REGISTER, 0x80);
        ppu.write_register(LYC_REGISTER, 0x01);
        ppu.write_register(
            STAT_REGISTER,
            STAT_MODE_0_INTERRUPT | STAT_COINCIDENCE_INTERRUPT,
        );

        for _ in 0..LCD_ENABLE_STARTUP_DELAY_DOTS {
            ppu.step(&mut interrupt_flag);
        }

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
            ppu.read_register(STAT_REGISTER).unwrap() & STAT_LYC_EQUAL,
            STAT_LYC_EQUAL
        );
        assert_eq!(interrupt_flag & INTERRUPT_STAT_BIT, 0);
    }

    #[test]
    fn background_pixel_fetch_uses_unsigned_tile_data_region() {
        let mut ppu = Ppu::default();
        ppu.write_register(LCDC_REGISTER, LCDC_BG_ENABLE | LCDC_BG_TILE_DATA_SELECT);

        ppu.write_vram(0x9800, 0x02);
        ppu.write_vram(0x8020, 0b1000_0000);
        ppu.write_vram(0x8021, 0b1000_0000);

        assert_eq!(ppu.background_pixel_color_id(0, 0), 3);
    }

    #[test]
    fn background_pixel_fetch_uses_signed_tile_data_region() {
        let mut ppu = Ppu::default();
        ppu.write_register(LCDC_REGISTER, LCDC_BG_ENABLE);

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
            LCDC_BG_ENABLE | LCDC_BG_TILE_MAP_SELECT | LCDC_BG_TILE_DATA_SELECT,
        );
        ppu.write_register(SCX_REGISTER, 8);
        ppu.write_register(SCY_REGISTER, 16);

        ppu.write_vram(0x9C41, 0x03);
        ppu.write_vram(0x8030, 0b0000_0000);
        ppu.write_vram(0x8031, 0b1000_0000);

        assert_eq!(ppu.background_pixel_color_id(0, 0), 2);
    }

    #[test]
    fn background_pixel_fetch_uses_window_when_positioned_on_screen() {
        let mut ppu = Ppu::default();
        ppu.write_register(
            LCDC_REGISTER,
            LCDC_BG_ENABLE
                | LCDC_WINDOW_ENABLE
                | LCDC_WINDOW_TILE_MAP_SELECT
                | LCDC_BG_TILE_DATA_SELECT,
        );
        ppu.write_register(WX_REGISTER, 7);
        ppu.write_register(WY_REGISTER, 0);

        ppu.write_vram(0x9C00, 0x04);
        ppu.write_vram(0x8040, 0b1000_0000);
        ppu.write_vram(0x8041, 0b1000_0000);

        assert_eq!(ppu.background_pixel_color_id(0, 0), 3);
    }

    #[test]
    fn background_pixel_fetch_ignores_window_when_hidden_by_position() {
        let mut ppu = Ppu::default();
        ppu.write_register(
            LCDC_REGISTER,
            LCDC_BG_ENABLE | LCDC_WINDOW_ENABLE | LCDC_BG_TILE_DATA_SELECT,
        );
        ppu.write_register(WX_REGISTER, 167);
        ppu.write_register(WY_REGISTER, 0);

        ppu.write_vram(0x9800, 0x01);
        ppu.write_vram(0x9C00, 0x02);
        ppu.write_vram(0x8010, 0b1000_0000);
        ppu.write_vram(0x8011, 0b0000_0000);
        ppu.write_vram(0x8020, 0b0000_0000);
        ppu.write_vram(0x8021, 0b1000_0000);

        assert_eq!(ppu.background_pixel_color_id(0, 0), 1);
    }

    #[test]
    fn sprite_pixel_uses_dmg_offsets_and_selects_obp0_or_obp1() {
        let mut ppu = Ppu::default();
        ppu.write_register(LCDC_REGISTER, LCDC_SPRITE_ENABLE);

        ppu.write_oam(0xFE00, 16);
        ppu.write_oam(0xFE01, 8);
        ppu.write_oam(0xFE02, 0x01);
        ppu.write_oam(0xFE03, 0x00);
        ppu.write_vram(0x8010, 0b1000_0000);
        ppu.write_vram(0x8011, 0b0000_0000);

        assert_eq!(
            ppu.sprite_pixel(0, 0, 0),
            Some(SpritePixel {
                color_id: 1,
                use_obp1: false,
                cgb_palette: 0
            })
        );

        ppu.write_oam(0xFE03, SPRITE_ATTRIBUTE_PALETTE);
        assert_eq!(
            ppu.sprite_pixel(0, 0, 0),
            Some(SpritePixel {
                color_id: 1,
                use_obp1: true,
                cgb_palette: 0
            })
        );
    }

    #[test]
    fn sprite_pixel_applies_x_y_flipping() {
        let mut ppu = Ppu::default();
        ppu.write_register(LCDC_REGISTER, LCDC_SPRITE_ENABLE);
        ppu.write_oam(0xFE00, 16);
        ppu.write_oam(0xFE01, 8);
        ppu.write_oam(0xFE02, 0x02);
        ppu.write_vram(0x8020, 0b0000_0001);
        ppu.write_vram(0x8021, 0b0000_0000);

        assert_eq!(
            ppu.sprite_pixel(7, 0, 0),
            Some(SpritePixel {
                color_id: 1,
                use_obp1: false,
                cgb_palette: 0
            })
        );

        ppu.write_oam(0xFE03, SPRITE_ATTRIBUTE_X_FLIP | SPRITE_ATTRIBUTE_Y_FLIP);
        ppu.write_vram(0x802E, 0b1000_0000);
        ppu.write_vram(0x802F, 0b0000_0000);
        assert_eq!(
            ppu.sprite_pixel(7, 0, 0),
            Some(SpritePixel {
                color_id: 1,
                use_obp1: false,
                cgb_palette: 0
            })
        );
    }

    #[test]
    fn sprite_pixel_honors_priority_and_oam_ordering_rules() {
        let mut ppu = Ppu::default();
        ppu.write_register(LCDC_REGISTER, LCDC_SPRITE_ENABLE | LCDC_BG_ENABLE);

        ppu.write_oam(0xFE00, 16);
        ppu.write_oam(0xFE01, 8);
        ppu.write_oam(0xFE02, 0x03);
        ppu.write_oam(0xFE03, SPRITE_ATTRIBUTE_PRIORITY);
        ppu.write_vram(0x8030, 0b1000_0000);
        ppu.write_vram(0x8031, 0b0000_0000);

        ppu.write_oam(0xFE04, 16);
        ppu.write_oam(0xFE05, 8);
        ppu.write_oam(0xFE06, 0x04);
        ppu.write_oam(0xFE07, 0x00);
        ppu.write_vram(0x8040, 0b1000_0000);
        ppu.write_vram(0x8041, 0b0000_0000);

        assert_eq!(ppu.sprite_pixel(0, 0, 2), None);
        assert_eq!(
            ppu.sprite_pixel(0, 0, 0),
            Some(SpritePixel {
                color_id: 1,
                use_obp1: false,
                cgb_palette: 0
            })
        );
    }

    #[test]
    fn sprite_pixel_supports_8x16_mode_and_ignores_lsb_of_tile_index() {
        let mut ppu = Ppu::default();
        ppu.write_register(LCDC_REGISTER, LCDC_SPRITE_ENABLE | LCDC_SPRITE_SIZE);
        ppu.write_oam(0xFE00, 16);
        ppu.write_oam(0xFE01, 8);
        ppu.write_oam(0xFE02, 0x03);
        // Top tile comes from index 0x02 (LSB ignored in 8x16 mode).
        ppu.write_vram(0x8020, 0b1000_0000);
        ppu.write_vram(0x8021, 0x00);
        // Bottom tile comes from index 0x03.
        ppu.write_vram(0x8030, 0x00);
        ppu.write_vram(0x8031, 0b1000_0000);

        assert_eq!(
            ppu.sprite_pixel(0, 0, 0),
            Some(SpritePixel {
                color_id: 1,
                use_obp1: false,
                cgb_palette: 0
            })
        );
        assert_eq!(
            ppu.sprite_pixel(0, 8, 0),
            Some(SpritePixel {
                color_id: 2,
                use_obp1: false,
                cgb_palette: 0
            })
        );
    }

    #[test]
    fn sprite_pixel_applies_y_flip_across_full_8x16_height() {
        let mut ppu = Ppu::default();
        ppu.write_register(LCDC_REGISTER, LCDC_SPRITE_ENABLE | LCDC_SPRITE_SIZE);
        ppu.write_oam(0xFE00, 16);
        ppu.write_oam(0xFE01, 8);
        ppu.write_oam(0xFE02, 0x02);
        ppu.write_oam(0xFE03, SPRITE_ATTRIBUTE_Y_FLIP);

        // Unflipped row 0 should sample from bottom tile row 7.
        ppu.write_vram(0x803E, 0b1000_0000);
        ppu.write_vram(0x803F, 0x00);

        assert_eq!(
            ppu.sprite_pixel(0, 0, 0),
            Some(SpritePixel {
                color_id: 1,
                use_obp1: false,
                cgb_palette: 0
            })
        );
    }

    #[test]
    fn sprite_pixel_does_not_leak_lower_priority_obj_behind_non_zero_bg() {
        let mut ppu = Ppu::default();
        ppu.write_register(LCDC_REGISTER, LCDC_SPRITE_ENABLE | LCDC_BG_ENABLE);

        // Higher-priority sprite in OAM order, masked by BG-over-OBJ when BG is non-zero.
        ppu.write_oam(0xFE00, 16);
        ppu.write_oam(0xFE01, 8);
        ppu.write_oam(0xFE02, 0x05);
        ppu.write_oam(0xFE03, SPRITE_ATTRIBUTE_PRIORITY);
        ppu.write_vram(0x8050, 0b1000_0000);
        ppu.write_vram(0x8051, 0b0000_0000);

        // Lower-priority overlapping sprite should not shine through in this case.
        ppu.write_oam(0xFE04, 16);
        ppu.write_oam(0xFE05, 8);
        ppu.write_oam(0xFE06, 0x06);
        ppu.write_oam(0xFE07, 0x00);
        ppu.write_vram(0x8060, 0b1000_0000);
        ppu.write_vram(0x8061, 0b0000_0000);

        assert_eq!(ppu.sprite_pixel(0, 0, 2), None);
        assert_eq!(
            ppu.sprite_pixel(0, 0, 0),
            Some(SpritePixel {
                color_id: 1,
                use_obp1: false,
                cgb_palette: 0
            })
        );
    }

    #[test]
    fn sprite_pixel_allows_lower_priority_sprite_through_transparent_winner_pixel() {
        let mut ppu = Ppu::default();
        ppu.write_register(LCDC_REGISTER, LCDC_SPRITE_ENABLE);

        // Lower-X sprite has higher DMG OBJ priority, but its pixel at (0, 0) is transparent.
        ppu.write_oam(0xFE00, 16);
        ppu.write_oam(0xFE01, 8);
        ppu.write_oam(0xFE02, 0x08);
        ppu.write_oam(0xFE03, 0x00);
        ppu.write_vram(0x8080, 0x00);
        ppu.write_vram(0x8081, 0x00);

        // Higher-X sprite still covers (0, 0) and should appear once the winner is transparent.
        ppu.write_oam(0xFE04, 16);
        ppu.write_oam(0xFE05, 7);
        ppu.write_oam(0xFE06, 0x09);
        ppu.write_oam(0xFE07, SPRITE_ATTRIBUTE_PALETTE);
        ppu.write_vram(0x8090, 0b0100_0000);
        ppu.write_vram(0x8091, 0x00);

        assert_eq!(
            ppu.sprite_pixel(0, 0, 0),
            Some(SpritePixel {
                color_id: 1,
                use_obp1: true,
                cgb_palette: 0
            })
        );
    }

    #[test]
    fn sprite_pixel_ignores_bg_over_obj_priority_when_bg_layer_is_disabled() {
        let mut ppu = Ppu::default();
        ppu.write_register(LCDC_REGISTER, LCDC_SPRITE_ENABLE);

        ppu.write_oam(0xFE00, 16);
        ppu.write_oam(0xFE01, 8);
        ppu.write_oam(0xFE02, 0x0A);
        ppu.write_oam(0xFE03, SPRITE_ATTRIBUTE_PRIORITY);
        ppu.write_vram(0x80A0, 0b1000_0000);
        ppu.write_vram(0x80A1, 0x00);

        assert_eq!(
            ppu.sprite_pixel(0, 0, 2),
            Some(SpritePixel {
                color_id: 1,
                use_obp1: false,
                cgb_palette: 0
            })
        );
    }

    #[test]
    fn sprite_pixel_limits_scanline_selection_to_first_10_oam_entries() {
        let mut ppu = Ppu::default();
        ppu.write_register(LCDC_REGISTER, LCDC_SPRITE_ENABLE);

        for sprite_index in 0..10usize {
            let base = 0xFE00 + (sprite_index as u16) * 4;
            ppu.write_oam(base, 16);
            ppu.write_oam(base + 1, 16);
            ppu.write_oam(base + 2, 0x00);
            ppu.write_oam(base + 3, 0x00);
        }

        // 11th sprite has a visible pixel at (0, 0) but should be ignored by scanline limit.
        ppu.write_oam(0xFE28, 16);
        ppu.write_oam(0xFE29, 8);
        ppu.write_oam(0xFE2A, 0x07);
        ppu.write_oam(0xFE2B, 0x00);
        ppu.write_vram(0x8070, 0b1000_0000);
        ppu.write_vram(0x8071, 0b0000_0000);

        assert_eq!(ppu.sprite_pixel(0, 0, 0), None);
    }

    #[test]
    fn dmg_palette_shade_decodes_each_color_slot() {
        let palette = 0b01_10_11_00;
        assert_eq!(dmg_palette_shade(palette, 0), 0);
        assert_eq!(dmg_palette_shade(palette, 1), 3);
        assert_eq!(dmg_palette_shade(palette, 2), 2);
        assert_eq!(dmg_palette_shade(palette, 3), 1);
    }

    #[test]
    fn composited_pixel_shade_applies_obj_palette_when_sprite_wins() {
        let mut ppu = Ppu::default();
        ppu.write_register(LCDC_REGISTER, LCDC_BG_ENABLE | LCDC_SPRITE_ENABLE);

        // Background tile 0 emits color id 0 at (0,0), which keeps sprite visible.
        ppu.write_vram(0x8000, 0x00);
        ppu.write_vram(0x8001, 0x00);
        ppu.write_register(BGP_REGISTER, 0b11_10_01_00);

        // Sprite at (0,0) emits color id 1 and selects OBP1.
        ppu.write_oam(0xFE00, 16);
        ppu.write_oam(0xFE01, 8);
        ppu.write_oam(0xFE02, 0x01);
        ppu.write_oam(0xFE03, SPRITE_ATTRIBUTE_PALETTE);
        ppu.write_vram(0x8010, 0b1000_0000);
        ppu.write_vram(0x8011, 0x00);

        ppu.write_register(OBP0_REGISTER, 0b00_00_00_00);
        ppu.write_register(OBP1_REGISTER, 0b00_00_10_00);
        ppu.write_register(
            LCDC_REGISTER,
            LCDC_BG_ENABLE | LCDC_SPRITE_ENABLE | LCDC_ENABLE,
        );

        assert_eq!(ppu.background_pixel_shade(0, 0), 0);
        assert_eq!(ppu.composited_pixel_shade(0, 0), 2);
    }

    #[test]
    fn composited_pixel_shade_returns_blank_when_lcd_disabled() {
        let mut ppu = Ppu::default();
        ppu.write_register(LCDC_REGISTER, LCDC_BG_ENABLE | LCDC_SPRITE_ENABLE);
        ppu.write_vram(0x8000, 0x00);
        ppu.write_vram(0x8001, 0x00);
        ppu.write_register(BGP_REGISTER, 0b11_10_01_00);
        ppu.write_oam(0xFE00, 16);
        ppu.write_oam(0xFE01, 8);
        ppu.write_oam(0xFE02, 0x01);
        ppu.write_oam(0xFE03, SPRITE_ATTRIBUTE_PALETTE);
        ppu.write_vram(0x8010, 0b1000_0000);
        ppu.write_vram(0x8011, 0x00);
        ppu.write_register(OBP1_REGISTER, 0b00_00_10_00);

        ppu.write_register(
            LCDC_REGISTER,
            LCDC_BG_ENABLE | LCDC_SPRITE_ENABLE | LCDC_ENABLE,
        );

        assert_eq!(ppu.composited_pixel_shade(0, 0), 2);

        ppu.write_register(LCDC_REGISTER, LCDC_BG_ENABLE | LCDC_SPRITE_ENABLE);
        assert_eq!(ppu.composited_pixel_shade(0, 0), 0);
    }

    #[test]
    fn background_pixel_shade_returns_blank_when_lcd_disabled() {
        let mut ppu = Ppu::default();
        ppu.write_register(LCDC_REGISTER, LCDC_BG_ENABLE);
        ppu.write_vram(0x8000, 0xFF);
        ppu.write_vram(0x8001, 0xFF);
        ppu.write_register(BGP_REGISTER, 0b11_10_01_00);

        ppu.write_register(LCDC_REGISTER, LCDC_BG_ENABLE | LCDC_ENABLE);

        ppu.write_register(LCDC_REGISTER, LCDC_BG_ENABLE);
        assert_eq!(ppu.background_pixel_shade(0, 0), 0);
    }

    #[test]
    fn background_pixel_shade_returns_white_when_bg_window_disabled() {
        let mut ppu = Ppu::default();
        ppu.write_register(LCDC_REGISTER, LCDC_BG_ENABLE | LCDC_ENABLE);
        ppu.write_vram(0x8000, 0x00);
        ppu.write_vram(0x8001, 0x00);
        ppu.write_register(BGP_REGISTER, 0b11_10_01_01);

        assert_eq!(ppu.background_pixel_shade(0, 0), 1);

        ppu.write_register(LCDC_REGISTER, LCDC_ENABLE);
        assert_eq!(ppu.background_pixel_shade(0, 0), 0);
    }

    #[test]
    fn composited_pixel_shade_returns_white_when_bg_window_disabled_and_no_sprite() {
        let mut ppu = Ppu::default();
        ppu.write_register(LCDC_REGISTER, LCDC_BG_ENABLE | LCDC_ENABLE);
        ppu.write_vram(0x8000, 0x00);
        ppu.write_vram(0x8001, 0x00);
        ppu.write_register(BGP_REGISTER, 0b11_10_01_01);

        assert_eq!(ppu.composited_pixel_shade(0, 0), 1);

        ppu.write_register(LCDC_REGISTER, LCDC_ENABLE);
        assert_eq!(ppu.composited_pixel_shade(0, 0), 0);
    }

    #[test]
    fn cgb_vbk_selects_independent_vram_banks() {
        let mut ppu = Ppu::new_cgb();

        ppu.write_vram(0x8000, 0x12);
        assert_eq!(ppu.read_vram(0x8000), 0x12);

        assert!(ppu.write_register(VBK_REGISTER, 0x01));
        assert_eq!(ppu.read_register(VBK_REGISTER), Some(0xFF));
        assert_eq!(ppu.selected_vram_bank(), 1);
        assert_eq!(ppu.read_vram(0x8000), 0x00);

        ppu.write_vram(0x8000, 0x34);
        assert_eq!(ppu.read_vram(0x8000), 0x34);

        ppu.write_register(VBK_REGISTER, 0x00);
        assert_eq!(ppu.selected_vram_bank(), 0);
        assert_eq!(ppu.read_vram(0x8000), 0x12);
    }

    #[test]
    fn dmg_mode_ignores_cgb_vbk_and_palette_register_writes() {
        let mut ppu = Ppu::default();

        assert_eq!(ppu.read_register(VBK_REGISTER), Some(0xFF));
        ppu.write_register(VBK_REGISTER, 0x01);
        ppu.write_register(BCPS_REGISTER, 0x80);
        ppu.write_register(BCPD_REGISTER, 0x56);
        ppu.write_register(OCPS_REGISTER, 0x80);
        ppu.write_register(OCPD_REGISTER, 0x78);

        assert_eq!(ppu.selected_vram_bank(), 0);
        assert_eq!(ppu.read_register(BCPS_REGISTER), Some(0xFF));
        assert_eq!(ppu.read_register(BCPD_REGISTER), Some(0xFF));
        assert_eq!(ppu.read_register(OCPS_REGISTER), Some(0xFF));
        assert_eq!(ppu.read_register(OCPD_REGISTER), Some(0xFF));
    }

    #[test]
    fn cgb_palette_data_ports_store_bytes_and_auto_increment_indices() {
        let mut ppu = Ppu::new_cgb();

        ppu.write_register(BCPS_REGISTER, 0x80 | 0x3F);
        ppu.write_register(BCPD_REGISTER, 0xAA);
        assert_eq!(ppu.read_register(BCPS_REGISTER), Some(0xC0));

        ppu.write_register(BCPS_REGISTER, 0x3F);
        assert_eq!(ppu.read_register(BCPD_REGISTER), Some(0xAA));

        ppu.write_register(OCPS_REGISTER, 0x80 | 0x02);
        ppu.write_register(OCPD_REGISTER, 0x11);
        ppu.write_register(OCPD_REGISTER, 0x22);
        assert_eq!(ppu.read_register(OCPS_REGISTER), Some(0xC4));

        ppu.write_register(OCPS_REGISTER, 0x02);
        assert_eq!(ppu.read_register(OCPD_REGISTER), Some(0x11));
        ppu.write_register(OCPS_REGISTER, 0x03);
        assert_eq!(ppu.read_register(OCPD_REGISTER), Some(0x22));
    }

    #[test]
    fn cgb_palette_data_ports_are_blocked_during_mode_3_without_index_increment() {
        let mut ppu = Ppu::new_cgb();

        ppu.write_register(BCPS_REGISTER, 0x80 | 0x05);
        ppu.write_register(BCPD_REGISTER, 0x12);
        ppu.write_register(BCPS_REGISTER, 0x80 | 0x05);
        ppu.stat.set_mode(PpuMode::from_bits(0x03));

        assert_eq!(ppu.read_register(BCPD_REGISTER), Some(0xFF));
        ppu.write_register(BCPD_REGISTER, 0x34);
        assert_eq!(ppu.read_register(BCPS_REGISTER), Some(0xC5));

        ppu.stat.set_mode(PpuMode::HBlank);
        assert_eq!(ppu.read_register(BCPD_REGISTER), Some(0x12));

        ppu.write_register(OCPS_REGISTER, 0x80 | 0x07);
        ppu.write_register(OCPD_REGISTER, 0x56);
        ppu.write_register(OCPS_REGISTER, 0x80 | 0x07);
        ppu.stat.set_mode(PpuMode::from_bits(0x03));

        assert_eq!(ppu.read_register(OCPD_REGISTER), Some(0xFF));
        ppu.write_register(OCPD_REGISTER, 0x78);
        assert_eq!(ppu.read_register(OCPS_REGISTER), Some(0xC7));

        ppu.stat.set_mode(PpuMode::HBlank);
        assert_eq!(ppu.read_register(OCPD_REGISTER), Some(0x56));
    }

    fn write_cgb_palette_color(
        ppu: &mut Ppu,
        index_register: u16,
        data_register: u16,
        offset: u8,
        rgb555: u16,
    ) {
        ppu.write_register(index_register, offset);
        ppu.write_register(data_register, (rgb555 & 0x00FF) as u8);
        ppu.write_register(index_register, offset + 1);
        ppu.write_register(data_register, (rgb555 >> 8) as u8);
    }

    #[test]
    fn cgb_background_rendering_uses_tile_attributes_and_palette_ram() {
        let mut ppu = Ppu::new_cgb();
        ppu.write_register(
            LCDC_REGISTER,
            LCDC_ENABLE | LCDC_BG_ENABLE | LCDC_BG_TILE_DATA_SELECT,
        );

        ppu.write_register(VBK_REGISTER, 0x00);
        ppu.write_vram(0x9800, 0x01);
        ppu.write_register(VBK_REGISTER, 0x01);
        ppu.write_vram(0x9800, CGB_BG_ATTR_VRAM_BANK | 0x03);
        ppu.write_vram(0x8010, 0x00);
        ppu.write_vram(0x8011, 0x80);
        write_cgb_palette_color(
            &mut ppu,
            BCPS_REGISTER,
            BCPD_REGISTER,
            3 * 8 + 2 * 2,
            0x1234,
        );

        assert_eq!(ppu.background_pixel_color_id(0, 0), 2);
        assert_eq!(ppu.composited_pixel_rgb555(0, 0), 0x1234);
    }

    #[test]
    fn cgb_lcdc_bg_priority_clear_still_renders_background_pixels() {
        let mut ppu = Ppu::new_cgb();
        ppu.write_register(VBK_REGISTER, 0x00);
        ppu.write_vram(0x9800, 0x01);
        ppu.write_vram(0x8010, 0x00);
        ppu.write_vram(0x8011, 0x80);
        write_cgb_palette_color(&mut ppu, BCPS_REGISTER, BCPD_REGISTER, 2 * 2, 0x4210);
        ppu.write_register(LCDC_REGISTER, LCDC_ENABLE | LCDC_BG_TILE_DATA_SELECT);

        assert_eq!(ppu.background_pixel_color_id(0, 0), 2);
        assert_eq!(ppu.composited_pixel_rgb555(0, 0), 0x4210);
    }

    #[test]
    fn cgb_lcdc_bg_priority_clear_lets_sprites_win_over_bg_priority() {
        let mut ppu = Ppu::new_cgb();
        ppu.write_register(VBK_REGISTER, 0x00);
        ppu.write_vram(0x9800, 0x01);
        ppu.write_register(VBK_REGISTER, 0x01);
        ppu.write_vram(0x9800, CGB_BG_ATTR_PRIORITY);
        ppu.write_register(VBK_REGISTER, 0x00);
        ppu.write_vram(0x8010, 0x80);
        ppu.write_vram(0x8011, 0x00);
        write_cgb_palette_color(&mut ppu, BCPS_REGISTER, BCPD_REGISTER, 2, 0x001F);

        ppu.write_oam(0xFE00, 16);
        ppu.write_oam(0xFE01, 8);
        ppu.write_oam(0xFE02, 0x02);
        ppu.write_oam(0xFE03, 0x00);
        ppu.write_vram(0x8020, 0x80);
        ppu.write_vram(0x8021, 0x80);
        write_cgb_palette_color(&mut ppu, OCPS_REGISTER, OCPD_REGISTER, 3 * 2, 0x7C00);
        ppu.write_register(
            LCDC_REGISTER,
            LCDC_ENABLE | LCDC_SPRITE_ENABLE | LCDC_BG_TILE_DATA_SELECT,
        );

        assert_eq!(ppu.composited_pixel_rgb555(0, 0), 0x7C00);
    }

    #[test]
    fn cgb_sprite_rendering_uses_vram_bank_and_object_palette_ram() {
        let mut ppu = Ppu::new_cgb();
        ppu.write_register(
            LCDC_REGISTER,
            LCDC_ENABLE | LCDC_BG_ENABLE | LCDC_SPRITE_ENABLE,
        );

        ppu.write_oam(0xFE00, 16);
        ppu.write_oam(0xFE01, 8);
        ppu.write_oam(0xFE02, 0x02);
        ppu.write_oam(0xFE03, CGB_OBJ_ATTR_VRAM_BANK | 0x05);
        ppu.write_register(VBK_REGISTER, 0x01);
        ppu.write_vram(0x8020, 0x80);
        ppu.write_vram(0x8021, 0x00);
        write_cgb_palette_color(&mut ppu, OCPS_REGISTER, OCPD_REGISTER, 5 * 8 + 2, 0x2A1F);

        assert_eq!(
            ppu.sprite_pixel(0, 0, 0),
            Some(SpritePixel {
                color_id: 1,
                use_obp1: false,
                cgb_palette: 5,
            })
        );
        assert_eq!(ppu.composited_pixel_rgb555(0, 0), 0x2A1F);
    }

    #[test]
    fn cgb_scanline_render_updates_rgb555_framebuffer() {
        let mut ppu = Ppu::new_cgb();
        ppu.write_register(VBK_REGISTER, 0x00);
        ppu.write_vram(0x9800, 0x01);
        ppu.write_vram(0x8010, 0x80);
        ppu.write_vram(0x8011, 0x80);
        write_cgb_palette_color(&mut ppu, BCPS_REGISTER, BCPD_REGISTER, 3 * 2, 0x03E0);
        ppu.write_register(BGP_REGISTER, 0xE4);
        ppu.write_register(
            LCDC_REGISTER,
            LCDC_ENABLE | LCDC_BG_ENABLE | LCDC_BG_TILE_DATA_SELECT,
        );

        ppu.render_visible_scanline(0);

        assert_eq!(ppu.framebuffer()[0], 3);
        assert_eq!(ppu.color_framebuffer()[0], 0x03E0);
        assert_eq!(ppu.color_framebuffer_pixels().len(), FRAMEBUFFER_LEN);
    }
}
