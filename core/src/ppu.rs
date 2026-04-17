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
const OAM_ENTRY_SIZE: usize = 4;
const OAM_SPRITE_COUNT: usize = OAM_SIZE / OAM_ENTRY_SIZE;
const MAX_SPRITES_PER_SCANLINE: usize = 10;
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
const LCDC_SPRITE_ENABLE_BIT: u8 = 0x02;
const LCDC_SPRITE_SIZE_BIT: u8 = 0x04;
const LCDC_WINDOW_ENABLE_BIT: u8 = 0x20;
const LCDC_WINDOW_TILE_MAP_SELECT_BIT: u8 = 0x40;
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
const SPRITE_ATTRIBUTE_PRIORITY_BIT: u8 = 0x80;
const SPRITE_ATTRIBUTE_Y_FLIP_BIT: u8 = 0x40;
const SPRITE_ATTRIBUTE_X_FLIP_BIT: u8 = 0x20;
const SPRITE_ATTRIBUTE_PALETTE_BIT: u8 = 0x10;

/// Resolves a 2-bit DMG palette shade (0-3) from a palette register and logical color id.
///
/// DMG palette registers (`BGP`, `OBP0`, `OBP1`) encode four 2-bit shade selectors:
/// - bits 1:0 map color id 0
/// - bits 3:2 map color id 1
/// - bits 5:4 map color id 2
/// - bits 7:6 map color id 3
pub fn dmg_palette_shade(palette: u8, color_id: u8) -> u8 {
    let shift = (color_id & 0x03) * 2;
    (palette >> shift) & 0x03
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpritePixel {
    pub color_id: u8,
    pub use_obp1: bool,
}

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
    fn window_map_base_offset(&self) -> usize {
        if (self.lcdc & LCDC_WINDOW_TILE_MAP_SELECT_BIT) != 0 {
            BG_MAP_1_OFFSET
        } else {
            BG_MAP_0_OFFSET
        }
    }

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
            let tile_offset = isize::from(signed_index) * 16;
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

    pub fn dma_read_vram(&self, address: u16) -> u8 {
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
    /// - Applies window positioning using `WX/WY` with the DMG `WX-7` rule when enabled.
    pub fn background_pixel_color_id(&self, screen_x: u8, screen_y: u8) -> u8 {
        if (self.lcdc & LCDC_BG_ENABLE_BIT) == 0 {
            return 0;
        }

        let window_visible = (self.lcdc & LCDC_WINDOW_ENABLE_BIT) != 0
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
        let row_in_tile = fetch_y % 8;
        let pixel_in_tile = 7 - (fetch_x % 8);

        let map_index = tile_row * 32 + tile_col;
        let tile_index = self.vram[map_base + map_index];
        let tile_row_offset = self.tile_data_row_offset(tile_index, row_in_tile);
        let low = self.vram[tile_row_offset];
        let high = self.vram[tile_row_offset + 1];

        let low_bit = (low >> pixel_in_tile) & 0x01;
        let high_bit = (high >> pixel_in_tile) & 0x01;
        (high_bit << 1) | low_bit
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
        if (self.lcdc & LCDC_SPRITE_ENABLE_BIT) == 0 {
            return None;
        }

        let sprite_height = if (self.lcdc & LCDC_SPRITE_SIZE_BIT) != 0 {
            16
        } else {
            8
        };

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

        let mut candidate: Option<(u8, usize, SpritePixel, u8)> = None;
        for sprite_index in scanline_sprites.into_iter().take(scanline_sprite_count) {
            let base = sprite_index * OAM_ENTRY_SIZE;
            let sprite_y = self.oam[base];
            let sprite_x = self.oam[base + 1];
            let tile_index = self.oam[base + 2];
            let attributes = self.oam[base + 3];

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
            if (attributes & SPRITE_ATTRIBUTE_Y_FLIP_BIT) != 0 {
                row = (sprite_height - 1) as u8 - row;
            }
            if (attributes & SPRITE_ATTRIBUTE_X_FLIP_BIT) != 0 {
                col = 7 - col;
            }

            let tile_id = if sprite_height == 16 {
                let top_tile = tile_index & 0xFE;
                top_tile.wrapping_add(row / 8)
            } else {
                tile_index
            };
            let row_in_tile = row & 0x07;

            let tile_row_offset =
                TILE_BLOCK_0_OFFSET + usize::from(tile_id) * 16 + usize::from(row_in_tile) * 2;
            let low = self.vram[tile_row_offset];
            let high = self.vram[tile_row_offset + 1];
            let bit = 7 - col;
            let color_id = (((high >> bit) & 1) << 1) | ((low >> bit) & 1);
            if color_id == 0 {
                continue;
            }

            let pixel = SpritePixel {
                color_id,
                use_obp1: (attributes & SPRITE_ATTRIBUTE_PALETTE_BIT) != 0,
            };

            match candidate {
                None => candidate = Some((sprite_x, sprite_index, pixel, attributes)),
                Some((best_x, best_index, _, _))
                    if sprite_x < best_x || (sprite_x == best_x && sprite_index < best_index) =>
                {
                    candidate = Some((sprite_x, sprite_index, pixel, attributes));
                }
                _ => {}
            }
        }

        candidate.and_then(|(_, _, pixel, attributes)| {
            if (attributes & SPRITE_ATTRIBUTE_PRIORITY_BIT) != 0 && bg_color_id != 0 {
                None
            } else {
                Some(pixel)
            }
        })
    }

    /// Returns the final DMG shade index (0-3) for the background/window layer.
    pub fn background_pixel_shade(&self, screen_x: u8, screen_y: u8) -> u8 {
        let color_id = self.background_pixel_color_id(screen_x, screen_y);
        dmg_palette_shade(self.bgp, color_id)
    }

    /// Returns the final DMG shade index (0-3) for the composited pixel at `(x, y)`.
    ///
    /// Sprite priority and transparency are resolved via [`Self::sprite_pixel`], then the
    /// selected BGP/OBP palette register is applied to obtain the framebuffer shade.
    pub fn composited_pixel_shade(&self, screen_x: u8, screen_y: u8) -> u8 {
        if (self.lcdc & LCDC_ENABLED_BIT) == 0 {
            return 0;
        }

        let bg_color_id = self.background_pixel_color_id(screen_x, screen_y);
        if let Some(sprite) = self.sprite_pixel(screen_x, screen_y, bg_color_id) {
            let palette = if sprite.use_obp1 {
                self.obp1
            } else {
                self.obp0
            };
            dmg_palette_shade(palette, sprite.color_id)
        } else {
            dmg_palette_shade(self.bgp, bg_color_id)
        }
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
    fn dma_write_oam_bypasses_mode_restrictions() {
        let mut ppu = Ppu::default();
        ppu.stat = (ppu.stat & !0x03) | 0x03;

        ppu.dma_write_oam(0, 0xAB);

        ppu.stat &= !0x03;
        assert_eq!(ppu.read_oam(0xFE00), 0xAB);
    }

    #[test]
    fn dma_reads_bypass_vram_and_oam_mode_restrictions() {
        let mut ppu = Ppu::default();
        ppu.write_vram(0x8000, 0x11);
        ppu.write_oam(0xFE00, 0x22);

        ppu.stat = (ppu.stat & !0x03) | 0x03;
        assert_eq!(ppu.read_vram(0x8000), 0xFF);
        assert_eq!(ppu.read_oam(0xFE00), 0xFF);

        assert_eq!(ppu.dma_read_vram(0x8000), 0x11);
        assert_eq!(ppu.dma_read_oam(0xFE00), 0x22);
    }

    #[test]
    fn sprite_pixel_uses_dma_written_oam_data_during_mode_3() {
        let mut ppu = Ppu::default();
        ppu.write_register(LCDC_REGISTER, LCDC_SPRITE_ENABLE_BIT);
        ppu.write_vram(0x8010, 0b1000_0000);
        ppu.write_vram(0x8011, 0x00);

        ppu.stat = (ppu.stat & !0x03) | 0x03;
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
                use_obp1: false
            })
        );
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

    #[test]
    fn background_pixel_fetch_uses_window_when_positioned_on_screen() {
        let mut ppu = Ppu::default();
        ppu.write_register(
            LCDC_REGISTER,
            LCDC_BG_ENABLE_BIT
                | LCDC_WINDOW_ENABLE_BIT
                | LCDC_WINDOW_TILE_MAP_SELECT_BIT
                | LCDC_BG_TILE_DATA_SELECT_BIT,
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
            LCDC_BG_ENABLE_BIT | LCDC_WINDOW_ENABLE_BIT | LCDC_BG_TILE_DATA_SELECT_BIT,
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
        ppu.write_register(LCDC_REGISTER, LCDC_SPRITE_ENABLE_BIT);

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
                use_obp1: false
            })
        );

        ppu.write_oam(0xFE03, SPRITE_ATTRIBUTE_PALETTE_BIT);
        assert_eq!(
            ppu.sprite_pixel(0, 0, 0),
            Some(SpritePixel {
                color_id: 1,
                use_obp1: true
            })
        );
    }

    #[test]
    fn sprite_pixel_applies_x_y_flipping() {
        let mut ppu = Ppu::default();
        ppu.write_register(LCDC_REGISTER, LCDC_SPRITE_ENABLE_BIT);
        ppu.write_oam(0xFE00, 16);
        ppu.write_oam(0xFE01, 8);
        ppu.write_oam(0xFE02, 0x02);
        ppu.write_vram(0x8020, 0b0000_0001);
        ppu.write_vram(0x8021, 0b0000_0000);

        assert_eq!(
            ppu.sprite_pixel(7, 0, 0),
            Some(SpritePixel {
                color_id: 1,
                use_obp1: false
            })
        );

        ppu.write_oam(
            0xFE03,
            SPRITE_ATTRIBUTE_X_FLIP_BIT | SPRITE_ATTRIBUTE_Y_FLIP_BIT,
        );
        ppu.write_vram(0x802E, 0b1000_0000);
        ppu.write_vram(0x802F, 0b0000_0000);
        assert_eq!(
            ppu.sprite_pixel(7, 0, 0),
            Some(SpritePixel {
                color_id: 1,
                use_obp1: false
            })
        );
    }

    #[test]
    fn sprite_pixel_honors_priority_and_oam_ordering_rules() {
        let mut ppu = Ppu::default();
        ppu.write_register(LCDC_REGISTER, LCDC_SPRITE_ENABLE_BIT);

        ppu.write_oam(0xFE00, 16);
        ppu.write_oam(0xFE01, 8);
        ppu.write_oam(0xFE02, 0x03);
        ppu.write_oam(0xFE03, SPRITE_ATTRIBUTE_PRIORITY_BIT);
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
                use_obp1: false
            })
        );
    }

    #[test]
    fn sprite_pixel_supports_8x16_mode_and_ignores_lsb_of_tile_index() {
        let mut ppu = Ppu::default();
        ppu.write_register(LCDC_REGISTER, LCDC_SPRITE_ENABLE_BIT | LCDC_SPRITE_SIZE_BIT);
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
                use_obp1: false
            })
        );
        assert_eq!(
            ppu.sprite_pixel(0, 8, 0),
            Some(SpritePixel {
                color_id: 2,
                use_obp1: false
            })
        );
    }

    #[test]
    fn sprite_pixel_applies_y_flip_across_full_8x16_height() {
        let mut ppu = Ppu::default();
        ppu.write_register(LCDC_REGISTER, LCDC_SPRITE_ENABLE_BIT | LCDC_SPRITE_SIZE_BIT);
        ppu.write_oam(0xFE00, 16);
        ppu.write_oam(0xFE01, 8);
        ppu.write_oam(0xFE02, 0x02);
        ppu.write_oam(0xFE03, SPRITE_ATTRIBUTE_Y_FLIP_BIT);

        // Unflipped row 0 should sample from bottom tile row 7.
        ppu.write_vram(0x803E, 0b1000_0000);
        ppu.write_vram(0x803F, 0x00);

        assert_eq!(
            ppu.sprite_pixel(0, 0, 0),
            Some(SpritePixel {
                color_id: 1,
                use_obp1: false
            })
        );
    }

    #[test]
    fn sprite_pixel_does_not_leak_lower_priority_obj_behind_non_zero_bg() {
        let mut ppu = Ppu::default();
        ppu.write_register(LCDC_REGISTER, LCDC_SPRITE_ENABLE_BIT);

        // Higher-priority sprite in OAM order, masked by BG-over-OBJ when BG is non-zero.
        ppu.write_oam(0xFE00, 16);
        ppu.write_oam(0xFE01, 8);
        ppu.write_oam(0xFE02, 0x05);
        ppu.write_oam(0xFE03, SPRITE_ATTRIBUTE_PRIORITY_BIT);
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
                use_obp1: false
            })
        );
    }

    #[test]
    fn sprite_pixel_limits_scanline_selection_to_first_10_oam_entries() {
        let mut ppu = Ppu::default();
        ppu.write_register(LCDC_REGISTER, LCDC_SPRITE_ENABLE_BIT);

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
        ppu.write_register(LCDC_REGISTER, LCDC_BG_ENABLE_BIT | LCDC_SPRITE_ENABLE_BIT);

        // Background tile 0 emits color id 0 at (0,0), which keeps sprite visible.
        ppu.write_vram(0x8000, 0x00);
        ppu.write_vram(0x8001, 0x00);
        ppu.write_register(BGP_REGISTER, 0b11_10_01_00);

        // Sprite at (0,0) emits color id 1 and selects OBP1.
        ppu.write_oam(0xFE00, 16);
        ppu.write_oam(0xFE01, 8);
        ppu.write_oam(0xFE02, 0x01);
        ppu.write_oam(0xFE03, SPRITE_ATTRIBUTE_PALETTE_BIT);
        ppu.write_vram(0x8010, 0b1000_0000);
        ppu.write_vram(0x8011, 0x00);

        ppu.write_register(OBP0_REGISTER, 0b00_00_00_00);
        ppu.write_register(OBP1_REGISTER, 0b00_00_10_00);
        ppu.write_register(
            LCDC_REGISTER,
            LCDC_BG_ENABLE_BIT | LCDC_SPRITE_ENABLE_BIT | LCDC_ENABLED_BIT,
        );

        assert_eq!(ppu.background_pixel_shade(0, 0), 0);
        assert_eq!(ppu.composited_pixel_shade(0, 0), 2);
    }

    #[test]
    fn composited_pixel_shade_returns_blank_when_lcd_disabled() {
        let mut ppu = Ppu::default();
        ppu.write_register(LCDC_REGISTER, LCDC_BG_ENABLE_BIT | LCDC_SPRITE_ENABLE_BIT);
        ppu.write_vram(0x8000, 0x00);
        ppu.write_vram(0x8001, 0x00);
        ppu.write_register(BGP_REGISTER, 0b11_10_01_00);
        ppu.write_oam(0xFE00, 16);
        ppu.write_oam(0xFE01, 8);
        ppu.write_oam(0xFE02, 0x01);
        ppu.write_oam(0xFE03, SPRITE_ATTRIBUTE_PALETTE_BIT);
        ppu.write_vram(0x8010, 0b1000_0000);
        ppu.write_vram(0x8011, 0x00);
        ppu.write_register(OBP1_REGISTER, 0b00_00_10_00);

        ppu.write_register(
            LCDC_REGISTER,
            LCDC_BG_ENABLE_BIT | LCDC_SPRITE_ENABLE_BIT | LCDC_ENABLED_BIT,
        );

        assert_eq!(ppu.composited_pixel_shade(0, 0), 2);

        ppu.write_register(
            LCDC_REGISTER,
            LCDC_BG_ENABLE_BIT | LCDC_SPRITE_ENABLE_BIT,
        );
        assert_eq!(ppu.composited_pixel_shade(0, 0), 0);
    }
}
