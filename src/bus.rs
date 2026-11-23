use crate::{apu::Apu, cartridge::Cartridge, ppu::Ppu, timer::Timer};

#[derive(Debug)]
struct Dma {
    register: u8,
    pending: Option<(u8, u8)>,
    source: u16,
    index: u8,
    phase: u8,
    tail: u8,
}

impl Default for Dma {
    fn default() -> Self {
        Self {
            register: 0xff,
            pending: None,
            source: 0,
            index: 160,
            phase: 0,
            tail: 0,
        }
    }
}

#[derive(Debug)]
pub struct Bus {
    pub cartridge: Cartridge,
    pub ppu: Ppu,
    pub apu: Apu,
    timer: Timer,
    wram: [u8; 0x2000],
    hram: [u8; 0x7f],
    ie: u8,
    interrupts: u8,
    dma: Dma,
    keys: u8,
    joy_select: u8,
    stop_wake: bool,
    sb: u8,
    sc: u8,
    serial_bits: u8,
    serial_outgoing: u8,
    serial: Vec<u8>,
    cycles: u64,
}

impl Bus {
    pub fn new(cartridge: Cartridge) -> Self {
        Self {
            cartridge,
            ppu: Ppu::default(),
            apu: Apu::default(),
            timer: Timer::new(0xabcc),
            wram: [0; 0x2000],
            hram: [0; 0x7f],
            ie: 0,
            interrupts: 1,
            dma: Dma::default(),
            keys: 0,
            joy_select: 0,
            stop_wake: false,
            sb: 0,
            sc: 0,
            serial_bits: 0,
            serial_outgoing: 0,
            serial: Vec::new(),
            cycles: 0,
        }
    }

    pub fn cycles(&self) -> u64 {
        self.cycles
    }
    pub fn serial(&self) -> &[u8] {
        &self.serial
    }
    pub fn clear_serial(&mut self) {
        self.serial.clear();
    }
    pub fn pending_interrupts(&self) -> u8 {
        self.ie & self.interrupts & 0x1f
    }

    pub fn ack_interrupt(&mut self, bit: u8) {
        self.interrupts &= !(1 << bit);
    }

    fn joy_lines(&self) -> u8 {
        let mut low = 15;
        if self.joy_select & 0x10 == 0 {
            low &= !self.keys & 15;
        }
        if self.joy_select & 0x20 == 0 {
            low &= !(self.keys >> 4);
        }
        low
    }

    fn joy_edge(&mut self, previous: u8) {
        if previous & !self.joy_lines() != 0 {
            self.interrupts |= 0x10;
            self.stop_wake = true;
        }
    }

    /// Pressed bits are Right, Left, Up, Down, A, B, Select and Start.
    pub fn set_keys(&mut self, pressed: u8) {
        let previous = self.joy_lines();
        self.keys = pressed;
        self.joy_edge(previous);
    }

    pub fn enter_stop(&mut self) -> bool {
        self.write_io(0xff04, 0);
        self.stop_wake = false;
        self.joy_lines() == 15
    }

    pub fn take_stop_wake(&mut self) -> bool {
        std::mem::take(&mut self.stop_wake)
    }

    pub fn stop_idle(&mut self) {
        self.cycles += 4;
        self.cartridge.tick(4);
    }

    /// Side-effect-free read for debugger and test protocols.
    pub fn peek(&self, address: u16) -> u8 {
        match address {
            0..=0x7fff | 0xa000..=0xbfff => self.cartridge.read(address),
            0x8000..=0x9fff => self.ppu.vram[usize::from(address - 0x8000)],
            0xc000..=0xfdff => self.wram[usize::from(address & 0x1fff)],
            0xfe00..=0xfe9f => self.ppu.oam[usize::from(address - 0xfe00)],
            0xfea0..=0xfeff => 0,
            0xff00 => 0xc0 | self.joy_select | self.joy_lines(),
            0xff01 => self.sb,
            0xff02 => 0x7e | self.sc,
            0xff04..=0xff07 => self.timer.read(address),
            0xff0f => 0xe0 | self.interrupts,
            0xff10..=0xff3f => self.apu.read(address),
            0xff46 => self.dma.register,
            0xff40..=0xff4b => self.ppu.read(address),
            0xff50 => 0xff,
            0xff80..=0xfffe => self.hram[usize::from(address - 0xff80)],
            0xffff => self.ie,
            _ => 0xff,
        }
    }

    fn write_io(&mut self, address: u16, value: u8) {
        match address {
            0..=0x7fff | 0xa000..=0xbfff => self.cartridge.write(address, value),
            0x8000..=0x9fff => self.ppu.vram[usize::from(address - 0x8000)] = value,
            0xc000..=0xfdff => self.wram[usize::from(address & 0x1fff)] = value,
            0xfe00..=0xfe9f => self.ppu.oam[usize::from(address - 0xfe00)] = value,
            0xff00 => {
                let previous = self.joy_lines();
                self.joy_select = value & 0x30;
                self.joy_edge(previous);
            }
            0xff01 => self.sb = value,
            0xff02 => {
                if self.sc & 0x80 == 0 || value & 0x80 == 0 {
                    self.serial_bits = 0;
                    self.serial_outgoing = 0;
                }
                self.sc = value & 0x81;
            }
            0xff04..=0xff07 => {
                let previous = self.timer.divider();
                self.timer.write(address, value);
                self.divider_edges(previous);
            }
            0xff0f => self.interrupts = value & 0x1f,
            0xff10..=0xff3f => self.apu.write(address, value),
            0xff46 => {
                self.dma.register = value;
                self.dma.pending = Some((value, 8));
            }
            0xff40..=0xff4b => self.interrupts |= self.ppu.write(address, value),
            0xff80..=0xfffe => self.hram[usize::from(address - 0xff80)] = value,
            0xffff => self.ie = value,
            _ => {}
        }
    }

    fn cpu_access(&self, address: u16, write: bool) -> bool {
        if self.dma.index < 160 || self.dma.tail != 0 {
            match address {
                0xfe00..=0xfeff => return false,
                0..=0xfdff
                    if ((0x8000..=0x9fff).contains(&address))
                        == ((0x8000..=0x9fff).contains(&self.dma.source)) =>
                {
                    return false
                }
                _ => {}
            }
        }
        match address {
            0x8000..=0x9fff if write => self.ppu.mode() != 3,
            0x8000..=0x9fff => self.ppu.vram_readable(),
            0xfe00..=0xfeff if write => self.ppu.oam_writable(),
            0xfe00..=0xfeff => self.ppu.oam_readable(),
            _ => true,
        }
    }

    pub fn cpu_read(&mut self, address: u16) -> u8 {
        let value = if self.cpu_access(address, false) {
            self.peek(address)
        } else {
            0xff
        };
        self.idle();
        value
    }

    pub fn cpu_write(&mut self, address: u16, value: u8) {
        if self.cpu_access(address, true) {
            self.write_io(address, value);
        }
        self.idle();
    }

    pub fn idle(&mut self) {
        self.cycles += 4;
        self.cartridge.tick(4);
        for _ in 0..4 {
            let previous = self.timer.divider();
            if self.timer.tick() {
                self.interrupts |= 4;
            }
            self.divider_edges(previous);
            self.interrupts |= self.ppu.tick();
            self.apu.tick();
            self.tick_dma();
        }
    }

    fn divider_edges(&mut self, previous: u16) {
        let div = self.timer.divider();
        let falling = previous & !div;
        if falling & 0x1000 != 0 {
            self.apu.clock_frame();
        }
        if falling & 0x0100 != 0 && self.sc == 0x81 {
            self.serial_clock(true);
        }
    }

    fn serial_clock(&mut self, incoming: bool) {
        self.serial_outgoing = (self.serial_outgoing << 1) | (self.sb >> 7);
        self.sb = (self.sb << 1) | u8::from(incoming);
        self.serial_bits += 1;
        if self.serial_bits == 8 {
            self.serial.push(self.serial_outgoing);
            self.serial_bits = 0;
            self.sc &= 1;
            self.interrupts |= 8;
        }
    }

    /// Clock one bit from an external serial peer, returning the transmitted bit.
    pub fn serial_external_clock(&mut self, incoming: bool) -> Option<bool> {
        if self.sc != 0x80 {
            return None;
        }
        let outgoing = self.sb & 0x80 != 0;
        self.serial_clock(incoming);
        Some(outgoing)
    }

    fn tick_dma(&mut self) {
        self.dma.tail = self.dma.tail.saturating_sub(1);
        if let Some((source, remaining)) = self.dma.pending {
            if remaining == 1 {
                let source = if source >= 0xe0 {
                    source - 0x20
                } else {
                    source
                };
                self.dma.source = u16::from(source) << 8;
                self.dma.index = 0;
                self.dma.phase = 3;
                self.dma.pending = None;
            } else {
                self.dma.pending = Some((source, remaining - 1));
            }
        }
        if self.dma.index < 160 {
            self.dma.phase = (self.dma.phase + 1) & 3;
            if self.dma.phase == 0 {
                let index = self.dma.index;
                let value = self.peek(self.dma.source + u16::from(index));
                self.ppu.oam[usize::from(index)] = value;
                self.dma.index += 1;
                if self.dma.index == 160 {
                    self.dma.tail = 4;
                }
            }
        }
    }
}
