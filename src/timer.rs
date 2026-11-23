//! Falling-edge timer with the four-T-state TIMA overflow interval.

#[derive(Debug)]
pub struct Timer {
    div: u16,
    tima: u8,
    tma: u8,
    tac: u8,
    pending: u8,
    reloaded: bool,
}

impl Timer {
    pub fn new(div: u16) -> Self {
        Self {
            div,
            tima: 0,
            tma: 0,
            tac: 0,
            pending: 0,
            reloaded: false,
        }
    }

    pub fn divider(&self) -> u16 {
        self.div
    }

    pub fn read(&self, address: u16) -> u8 {
        match address {
            0xff04 => (self.div >> 8) as u8,
            0xff05 => self.tima,
            0xff06 => self.tma,
            0xff07 => 0xf8 | self.tac,
            _ => unreachable!(),
        }
    }

    fn signal(&self) -> bool {
        let bit = [9, 3, 5, 7][usize::from(self.tac & 3)];
        self.tac & 4 != 0 && self.div & (1 << bit) != 0
    }

    fn falling_edge(&mut self, previous: bool) {
        if previous && !self.signal() && self.pending == 0 && !self.reloaded {
            self.tima = self.tima.wrapping_add(1);
            if self.tima == 0 {
                self.pending = 4;
            }
        }
    }

    pub fn tick(&mut self) -> bool {
        self.reloaded = false;
        if self.pending != 0 {
            self.pending -= 1;
            if self.pending == 0 {
                self.tima = self.tma;
                self.reloaded = true;
            }
        }
        let previous = self.signal();
        self.div = self.div.wrapping_add(1);
        self.falling_edge(previous);
        self.reloaded
    }

    pub fn write(&mut self, address: u16, value: u8) {
        let previous = self.signal();
        match address {
            0xff04 => self.div = 0,
            0xff05 if !self.reloaded => {
                self.tima = value;
                self.pending = 0;
            }
            0xff05 => {}
            0xff06 => {
                self.tma = value;
                if self.reloaded {
                    self.tima = value;
                }
            }
            0xff07 => self.tac = value & 7,
            _ => unreachable!(),
        }
        self.falling_edge(previous);
    }
}
