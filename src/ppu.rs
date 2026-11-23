pub const WIDTH: usize = 160;
pub const HEIGHT: usize = 144;
pub const FRAME_PIXELS: usize = WIDTH * HEIGHT;

#[derive(Clone, Copy, Debug, Default)]
struct Sprite {
    y: u8,
    x: u8,
    tile: u8,
    flags: u8,
}

#[derive(Debug)]
pub struct Ppu {
    pub(crate) vram: [u8; 0x2000],
    pub(crate) oam: [u8; 160],
    framebuffer: [u8; FRAME_PIXELS],
    lcdc: u8,
    stat: u8,
    scy: u8,
    scx: u8,
    ly: u8,
    lyc: u8,
    bgp: u8,
    obp: [u8; 2],
    wy: u8,
    wx: u8,
    mode: u8,
    dot: u16,
    line: u8,
    stat_line: bool,
    coincidence: bool,
    lyc_line: bool,
    first_line: bool,
    frames: u64,
    sprites: [Sprite; 10],
    sprite_count: usize,
    sprite_cursor: usize,
    last_sprite_tile: i16,
    output_x: usize,
    delay: u8,
    hblank_delay: u8,
    window_y_trigger: bool,
    window_active: bool,
    window_drew: bool,
    window_line: u8,
    window_origin: i16,
}

impl Default for Ppu {
    fn default() -> Self {
        Self {
            vram: [0; 0x2000],
            oam: [0; 160],
            framebuffer: [0; FRAME_PIXELS],
            lcdc: 0x91,
            stat: 0,
            scy: 0,
            scx: 0,
            ly: 0,
            lyc: 0,
            bgp: 0xfc,
            obp: [0xff; 2],
            wy: 0,
            wx: 0,
            mode: 2,
            dot: 4,
            line: 0,
            stat_line: false,
            coincidence: true,
            lyc_line: true,
            first_line: false,
            frames: 0,
            sprites: [Sprite::default(); 10],
            sprite_count: 0,
            sprite_cursor: 0,
            last_sprite_tile: -2,
            output_x: 0,
            delay: 0,
            hblank_delay: 0,
            window_y_trigger: true,
            window_active: false,
            window_drew: false,
            window_line: 0,
            window_origin: 0,
        }
    }
}

impl Ppu {
    pub fn framebuffer(&self) -> &[u8; FRAME_PIXELS] {
        &self.framebuffer
    }
    pub fn frames(&self) -> u64 {
        self.frames
    }
    pub fn mode(&self) -> u8 {
        self.mode
    }
    pub fn dot(&self) -> u16 {
        self.dot
    }
    pub fn enabled(&self) -> bool {
        self.lcdc & 0x80 != 0
    }

    pub(crate) fn oam_writable(&self) -> bool {
        self.mode < 2 || (self.mode == 2 && self.dot >= 80)
    }

    pub(crate) fn oam_readable(&self) -> bool {
        self.mode < 2 && !(self.enabled() && !self.first_line && self.line < 144 && self.dot < 4)
    }

    pub(crate) fn vram_readable(&self) -> bool {
        self.mode != 3 && !(self.mode == 2 && self.dot >= 80)
    }

    pub fn read(&self, address: u16) -> u8 {
        match address {
            0xff40 => self.lcdc,
            0xff41 => 0x80 | self.stat | (u8::from(self.coincidence) << 2) | self.mode,
            0xff42 => self.scy,
            0xff43 => self.scx,
            0xff44 => self.ly,
            0xff45 => self.lyc,
            0xff47 => self.bgp,
            0xff48 => self.obp[0],
            0xff49 => self.obp[1],
            0xff4a => self.wy,
            0xff4b => self.wx,
            _ => unreachable!(),
        }
    }

    fn stat_edge(&mut self, enables: u8) -> u8 {
        if !self.enabled() {
            return 0;
        }
        let line_start = !self.first_line && self.line < 144 && self.dot < 4;
        if line_start && self.line != 0 {
            self.coincidence = false;
        } else {
            self.coincidence = self.ly == self.lyc;
            self.lyc_line = self.coincidence;
        }
        let hblank = self.mode == 0
            && self.hblank_delay == 0
            && !(line_start && self.line == 0)
            && (!self.first_line || self.output_x == WIDTH);
        let oam_pulse = !self.first_line && self.line < 144 && (3..=4).contains(&self.dot);
        let vblank_pulse = self.ly == 144 && self.dot == 0;
        let vblank = self.mode == 1 && (self.line != 144 || self.dot >= 4);
        let level = (hblank && enables & 8 != 0)
            || (vblank && enables & 0x10 != 0)
            || ((oam_pulse || vblank_pulse) && enables & 0x20 != 0)
            || (self.lyc_line && enables & 0x40 != 0);
        let interrupt = level && !self.stat_line;
        self.stat_line = level;
        if interrupt {
            2
        } else {
            0
        }
    }

    pub fn write(&mut self, address: u16, value: u8) -> u8 {
        let mut interrupts = 0;
        match address {
            0xff40 => {
                let enabled = self.enabled();
                self.lcdc = value;
                if enabled != self.enabled() {
                    self.line = 0;
                    self.ly = 0;
                    self.dot = 0;
                    self.mode = 0;
                    self.first_line = self.enabled();
                    self.window_line = 0;
                    self.window_y_trigger = false;
                    self.output_x = 0;
                    self.hblank_delay = 0;
                    self.begin_line();
                    if !self.enabled() {
                        self.framebuffer.fill(0);
                    }
                }
            }
            0xff41 => {
                // DMG briefly decodes all STAT enable bits during a write.
                interrupts |= self.stat_edge(0x78);
                self.stat = value & 0x78;
            }
            0xff42 => self.scy = value,
            0xff43 => self.scx = value,
            0xff44 => {}
            0xff45 => self.lyc = value,
            0xff47 => self.bgp = value,
            0xff48 => self.obp[0] = value,
            0xff49 => self.obp[1] = value,
            0xff4a => {
                self.wy = value;
                if self.enabled() && self.mode == 2 && self.ly == self.wy {
                    self.window_y_trigger = true;
                }
            }
            0xff4b => self.wx = value,
            _ => unreachable!(),
        }
        interrupts | self.stat_edge(self.stat)
    }

    fn begin_line(&mut self) {
        self.sprite_count = 0;
        self.sprite_cursor = 0;
        self.last_sprite_tile = -2;
        self.window_active = false;
        self.window_drew = false;
        self.window_y_trigger |= self.ly == self.wy;
    }

    fn scan_sprite(&mut self, index: usize) {
        if self.sprite_count == self.sprites.len() {
            return;
        }
        let base = index * 4;
        let y = i16::from(self.oam[base]) - 16;
        let height = if self.lcdc & 4 == 0 { 8 } else { 16 };
        if (y..y + height).contains(&i16::from(self.ly)) {
            self.sprites[self.sprite_count] = Sprite {
                y: self.oam[base],
                x: self.oam[base + 1],
                tile: self.oam[base + 2],
                flags: self.oam[base + 3],
            };
            self.sprite_count += 1;
        }
    }

    fn begin_drawing(&mut self) {
        // Stable insertion order for equal X means lower OAM index wins.
        let sprites = &mut self.sprites[..self.sprite_count];
        for i in 1..sprites.len() {
            let sprite = sprites[i];
            let mut j = i;
            while j > 0 && sprites[j - 1].x > sprite.x {
                sprites[j] = sprites[j - 1];
                j -= 1;
            }
            sprites[j] = sprite;
        }
        self.mode = 3;
        self.output_x = 0;
        self.delay = if self.first_line { 13 } else { 10 } + (self.scx & 7);
    }

    fn tile_pixel(&self, base: usize, x: u8) -> u8 {
        let bit = 7 - (x & 7);
        (self.vram[base] >> bit & 1) | ((self.vram[base + 1] >> bit & 1) << 1)
    }

    fn background_pixel(&self, x: u8, y: u8, high_map: bool) -> u8 {
        let map = if high_map { 0x1c00 } else { 0x1800 };
        let tile = self.vram[map + usize::from(y / 8) * 32 + usize::from(x / 8)];
        let index = if self.lcdc & 0x10 != 0 {
            usize::from(tile)
        } else {
            (256 + i16::from(tile as i8)) as usize
        };
        self.tile_pixel(index * 16 + usize::from(y & 7) * 2, x)
    }

    fn sprite_pixel(&self, x: i16) -> Option<(u8, u8)> {
        let height = if self.lcdc & 4 == 0 { 8 } else { 16 };
        for sprite in &self.sprites[..self.sprite_count] {
            let left = i16::from(sprite.x) - 8;
            if !(left..left + 8).contains(&x) {
                continue;
            }
            let mut column = (x - left) as u8;
            let mut row = (i16::from(self.ly) + 16 - i16::from(sprite.y)) as u8;
            if row >= height {
                continue;
            }
            if sprite.flags & 0x20 != 0 {
                column ^= 7;
            }
            if sprite.flags & 0x40 != 0 {
                row ^= height - 1;
            }
            let tile = if height == 16 {
                sprite.tile & 0xfe
            } else {
                sprite.tile
            };
            let color = self.tile_pixel(usize::from(tile) * 16 + usize::from(row) * 2, column);
            if color != 0 {
                return Some((color, sprite.flags));
            }
        }
        None
    }

    fn draw_dot(&mut self) {
        if self.delay != 0 {
            self.delay -= 1;
            return;
        }
        let x = self.output_x as i16;
        let window_x = i16::from(self.wx) - 7;
        if !self.window_active
            && self.window_y_trigger
            && self.lcdc & 0x21 == 0x21
            && self.wx <= 166
            && x == window_x.max(0)
        {
            self.window_active = true;
            self.window_origin = window_x;
            self.last_sprite_tile = -2;
            self.delay = 5;
            return;
        }
        while self.sprite_cursor < self.sprite_count {
            let sprite = self.sprites[self.sprite_cursor];
            let left = i16::from(sprite.x) - 8;
            if left > x {
                break;
            }
            self.sprite_cursor += 1;
            if self.lcdc & 2 != 0 {
                let offset = if self.window_active {
                    -self.window_origin
                } else {
                    i16::from(self.scx)
                };
                let aligned = i16::from(sprite.x) + offset;
                let tile = aligned.div_euclid(8);
                let align_penalty = if tile == self.last_sprite_tile {
                    0
                } else if sprite.x == 0 {
                    5
                } else {
                    5 - aligned.rem_euclid(8).min(5) as u8
                };
                self.last_sprite_tile = tile;
                self.delay = 5 + align_penalty;
                return;
            }
        }

        let bg_color = if self.lcdc & 1 == 0 {
            0
        } else if self.window_active && self.lcdc & 0x20 != 0 {
            self.window_drew = true;
            self.background_pixel(
                (x - self.window_origin) as u8,
                self.window_line,
                self.lcdc & 0x40 != 0,
            )
        } else {
            self.background_pixel(
                (x as u8).wrapping_add(self.scx),
                self.ly.wrapping_add(self.scy),
                self.lcdc & 8 != 0,
            )
        };
        let mut shade = if self.lcdc & 1 != 0 {
            (self.bgp >> (bg_color * 2)) & 3
        } else {
            0
        };
        if self.lcdc & 2 != 0 {
            if let Some((color, flags)) = self.sprite_pixel(x) {
                if bg_color == 0 || flags & 0x80 == 0 {
                    shade = (self.obp[usize::from(flags >> 4 & 1)] >> (color * 2)) & 3;
                }
            }
        }
        self.framebuffer[usize::from(self.ly) * WIDTH + self.output_x] = shade;
        self.output_x += 1;
        if self.output_x == WIDTH {
            self.mode = 0;
            self.hblank_delay = 3;
        }
    }

    pub fn tick(&mut self) -> u8 {
        if !self.enabled() {
            return 0;
        }
        self.dot += 1;
        self.hblank_delay = self.hblank_delay.saturating_sub(1);
        let mut interrupts = 0;
        if self.line < 144 {
            let drawing_start = if self.first_line { 80 } else { 84 };
            if !self.first_line && self.dot == 4 {
                self.mode = 2;
            }
            if self.mode == 2 && self.dot > 4 && self.dot <= drawing_start && self.dot & 1 == 0 {
                self.scan_sprite(usize::from((self.dot - 6) / 2));
            }
            if self.dot == drawing_start {
                self.begin_drawing();
            }
            if self.mode == 3 {
                self.draw_dot();
            }
        } else if self.line == 153 && self.dot == 4 {
            self.ly = 0;
        }
        if self.dot == if self.first_line { 452 } else { 456 } {
            self.dot = 0;
            self.first_line = false;
            if self.window_drew {
                self.window_line = self.window_line.wrapping_add(1);
            }
            self.line = if self.line == 153 { 0 } else { self.line + 1 };
            self.ly = self.line;
            match self.line {
                0..=143 => {
                    if self.line == 0 {
                        self.window_y_trigger = false;
                        self.window_line = 0;
                    }
                    self.mode = 0;
                    self.begin_line();
                }
                144 => {
                    self.mode = 1;
                    self.frames += 1;
                    interrupts |= 1;
                    self.window_drew = false;
                }
                _ => {}
            }
        }
        interrupts | self.stat_edge(self.stat)
    }
}
