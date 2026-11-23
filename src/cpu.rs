//! SM83 decoder. Each bus operation or internal idle advances one machine cycle.

use crate::bus::Bus;

const Z: u8 = 0x80;
const N: u8 = 0x40;
const H: u8 = 0x20;
const C: u8 = 0x10;

fn flag(condition: bool, mask: u8) -> u8 {
    if condition {
        mask
    } else {
        0
    }
}

#[derive(Debug, Clone)]
pub struct Cpu {
    pub a: u8,
    pub f: u8,
    pub b: u8,
    pub c: u8,
    pub d: u8,
    pub e: u8,
    pub h: u8,
    pub l: u8,
    pub sp: u16,
    pub pc: u16,
    pub ime: bool,
    pub halted: bool,
    pub stopped: bool,
    pub locked: bool,
    halt_bug: bool,
    ei_pending: bool,
}

impl Default for Cpu {
    fn default() -> Self {
        Self {
            a: 1,
            f: 0xb0,
            b: 0,
            c: 0x13,
            d: 0,
            e: 0xd8,
            h: 1,
            l: 0x4d,
            sp: 0xfffe,
            pc: 0x100,
            ime: false,
            halted: false,
            stopped: false,
            locked: false,
            halt_bug: false,
            ei_pending: false,
        }
    }
}

impl Cpu {
    pub fn bc(&self) -> u16 {
        u16::from_be_bytes([self.b, self.c])
    }
    pub fn de(&self) -> u16 {
        u16::from_be_bytes([self.d, self.e])
    }
    pub fn hl(&self) -> u16 {
        u16::from_be_bytes([self.h, self.l])
    }
    pub fn af(&self) -> u16 {
        u16::from_be_bytes([self.a, self.f])
    }

    fn set_hl(&mut self, value: u16) {
        [self.h, self.l] = value.to_be_bytes();
    }

    fn rp(&self, index: u8) -> u16 {
        match index {
            0 => self.bc(),
            1 => self.de(),
            2 => self.hl(),
            _ => self.sp,
        }
    }

    fn set_rp(&mut self, index: u8, value: u16) {
        match index {
            0 => [self.b, self.c] = value.to_be_bytes(),
            1 => [self.d, self.e] = value.to_be_bytes(),
            2 => self.set_hl(value),
            _ => self.sp = value,
        }
    }

    fn r(&self, index: u8, bus: &mut Bus) -> u8 {
        match index {
            0 => self.b,
            1 => self.c,
            2 => self.d,
            3 => self.e,
            4 => self.h,
            5 => self.l,
            6 => bus.cpu_read(self.hl()),
            _ => self.a,
        }
    }

    fn set_r(&mut self, index: u8, value: u8, bus: &mut Bus) {
        match index {
            0 => self.b = value,
            1 => self.c = value,
            2 => self.d = value,
            3 => self.e = value,
            4 => self.h = value,
            5 => self.l = value,
            6 => bus.cpu_write(self.hl(), value),
            _ => self.a = value,
        }
    }

    fn immediate(&mut self, bus: &mut Bus) -> u8 {
        let value = bus.cpu_read(self.pc);
        self.pc = self.pc.wrapping_add(1);
        value
    }

    fn immediate16(&mut self, bus: &mut Bus) -> u16 {
        let low = self.immediate(bus);
        let high = self.immediate(bus);
        u16::from_le_bytes([low, high])
    }

    fn push(&mut self, bus: &mut Bus, value: u16) {
        let [high, low] = value.to_be_bytes();
        self.sp = self.sp.wrapping_sub(1);
        bus.cpu_write(self.sp, high);
        self.sp = self.sp.wrapping_sub(1);
        bus.cpu_write(self.sp, low);
    }

    fn pop(&mut self, bus: &mut Bus) -> u16 {
        let low = bus.cpu_read(self.sp);
        self.sp = self.sp.wrapping_add(1);
        let high = bus.cpu_read(self.sp);
        self.sp = self.sp.wrapping_add(1);
        u16::from_le_bytes([low, high])
    }

    fn condition(&self, index: u8) -> bool {
        let mask = if index < 2 { Z } else { C };
        (self.f & mask != 0) == (index & 1 != 0)
    }

    fn alu(&mut self, operation: u8, value: u8) {
        let a = self.a;
        match operation {
            0 | 1 => {
                let carry = u8::from(operation == 1 && self.f & C != 0);
                let sum = u16::from(a) + u16::from(value) + u16::from(carry);
                self.a = sum as u8;
                self.f = flag(self.a == 0, Z)
                    | flag((a & 15) + (value & 15) + carry > 15, H)
                    | flag(sum > 255, C);
            }
            2 | 3 | 7 => {
                let carry = u8::from(operation == 3 && self.f & C != 0);
                let result = a.wrapping_sub(value).wrapping_sub(carry);
                self.f = N
                    | flag(result == 0, Z)
                    | flag((a & 15) < (value & 15) + carry, H)
                    | flag(u16::from(a) < u16::from(value) + u16::from(carry), C);
                if operation != 7 {
                    self.a = result;
                }
            }
            4..=6 => {
                self.a = match operation {
                    4 => a & value,
                    5 => a ^ value,
                    _ => a | value,
                };
                self.f = flag(self.a == 0, Z) | flag(operation == 4, H);
            }
            _ => unreachable!(),
        }
    }

    fn add_sp_offset(&mut self, offset: u8) -> u16 {
        self.f = flag((self.sp & 15) + u16::from(offset & 15) > 15, H)
            | flag((self.sp & 255) + u16::from(offset) > 255, C);
        self.sp.wrapping_add_signed(i16::from(offset as i8))
    }

    fn daa(&mut self) {
        let subtract = self.f & N != 0;
        let carry = self.f & C != 0 || (!subtract && self.a > 0x99);
        let half = self.f & H != 0 || (!subtract && self.a & 15 > 9);
        let adjust = (if carry { 0x60 } else { 0 }) | (if half { 6 } else { 0 });
        self.a = if subtract {
            self.a.wrapping_sub(adjust)
        } else {
            self.a.wrapping_add(adjust)
        };
        self.f = (self.f & N) | flag(self.a == 0, Z) | flag(carry, C);
    }

    fn rotate(&mut self, index: u8, value: u8) -> u8 {
        let carry_in = u8::from(self.f & C != 0);
        let (result, carry) = match index {
            0 => (value.rotate_left(1), value >> 7),
            1 => (value.rotate_right(1), value & 1),
            2 => ((value << 1) | carry_in, value >> 7),
            3 => ((value >> 1) | (carry_in << 7), value & 1),
            4 => (value << 1, value >> 7),
            5 => ((value >> 1) | (value & 0x80), value & 1),
            6 => (value.rotate_left(4), 0),
            _ => (value >> 1, value & 1),
        };
        self.f = flag(result == 0, Z) | flag(carry != 0, C);
        result
    }

    fn cb(&mut self, bus: &mut Bus) {
        let opcode = self.immediate(bus);
        let index = opcode & 7;
        let bit = (opcode >> 3) & 7;
        let value = self.r(index, bus);
        match opcode >> 6 {
            0 => {
                let result = self.rotate(bit, value);
                self.set_r(index, result, bus);
            }
            1 => self.f = (self.f & C) | H | flag(value & (1 << bit) == 0, Z),
            2 => self.set_r(index, value & !(1 << bit), bus),
            _ => self.set_r(index, value | (1 << bit), bus),
        }
    }

    fn relative_jump(&mut self, bus: &mut Bus, offset: u8) {
        bus.idle();
        self.pc = self.pc.wrapping_add_signed(i16::from(offset as i8));
    }

    fn return_address(&mut self, bus: &mut Bus) {
        self.pc = self.pop(bus);
        bus.idle();
    }

    fn interrupt(&mut self, bus: &mut Bus) {
        self.ime = false;
        self.ei_pending = false;
        self.halt_bug = false;
        bus.idle();
        bus.idle();
        self.sp = self.sp.wrapping_sub(1);
        bus.cpu_write(self.sp, (self.pc >> 8) as u8);
        // An interrupt can be cancelled, for example when pushing over IE.
        let pending = bus.pending_interrupts();
        let bit = pending.trailing_zeros();
        self.sp = self.sp.wrapping_sub(1);
        bus.cpu_write(self.sp, self.pc as u8);
        if bit < 5 {
            bus.ack_interrupt(bit as u8);
            self.pc = 0x40 + 8 * bit as u16;
        } else {
            self.pc = 0;
        }
        bus.idle();
    }

    pub fn step(&mut self, bus: &mut Bus) {
        if self.locked {
            bus.idle();
            return;
        }
        if self.stopped {
            if !bus.take_stop_wake() {
                bus.stop_idle();
                return;
            }
            self.stopped = false;
        }
        let pending = bus.pending_interrupts();
        if self.halted && pending == 0 {
            bus.idle();
            return;
        }
        self.halted = false;
        if self.ime && pending != 0 {
            self.interrupt(bus);
            return;
        }

        let enable_ime = std::mem::take(&mut self.ei_pending);
        let opcode = bus.cpu_read(self.pc);
        if !std::mem::take(&mut self.halt_bug) {
            self.pc = self.pc.wrapping_add(1);
        }
        self.execute(opcode, bus);
        if enable_ime && opcode != 0xf3 {
            self.ime = true;
        }
    }

    fn execute(&mut self, opcode: u8, bus: &mut Bus) {
        let x = opcode >> 6;
        let y = (opcode >> 3) & 7;
        let z = opcode & 7;
        let p = y >> 1;
        let q = y & 1;
        match x {
            0 => self.group_zero(y, z, p, q, bus),
            1 if opcode == 0x76 => {
                if !self.ime && bus.pending_interrupts() != 0 {
                    self.halt_bug = true;
                } else {
                    self.halted = true;
                }
            }
            1 => {
                let value = self.r(z, bus);
                self.set_r(y, value, bus);
            }
            2 => {
                let value = self.r(z, bus);
                self.alu(y, value);
            }
            _ => self.group_three(y, z, p, q, bus),
        }
    }

    fn group_zero(&mut self, y: u8, z: u8, p: u8, q: u8, bus: &mut Bus) {
        match z {
            0 => match y {
                0 => {}
                1 => {
                    let address = self.immediate16(bus);
                    let [low, high] = self.sp.to_le_bytes();
                    bus.cpu_write(address, low);
                    bus.cpu_write(address.wrapping_add(1), high);
                }
                2 => {
                    self.pc = self.pc.wrapping_add(1);
                    self.stopped = bus.enter_stop();
                }
                _ => {
                    let offset = self.immediate(bus);
                    if y == 3 || self.condition(y - 4) {
                        self.relative_jump(bus, offset);
                    }
                }
            },
            1 if q == 0 => {
                let value = self.immediate16(bus);
                self.set_rp(p, value);
            }
            1 => {
                let hl = self.hl();
                let rhs = self.rp(p);
                let (sum, carry) = hl.overflowing_add(rhs);
                bus.idle();
                self.set_hl(sum);
                self.f = (self.f & Z)
                    | flag((hl & 0x0fff) + (rhs & 0x0fff) > 0x0fff, H)
                    | flag(carry, C);
            }
            2 => {
                let address = self.rp(p.min(2));
                if q == 0 {
                    bus.cpu_write(address, self.a);
                } else {
                    self.a = bus.cpu_read(address);
                }
                if p >= 2 {
                    self.set_hl(address.wrapping_add_signed(if p == 2 { 1 } else { -1 }));
                }
            }
            3 => {
                let delta = if q == 0 { 1 } else { -1 };
                bus.idle();
                self.set_rp(p, self.rp(p).wrapping_add_signed(delta));
            }
            4 | 5 => {
                let value = self.r(y, bus);
                let subtract = z == 5;
                let result = value.wrapping_add_signed(if subtract { -1 } else { 1 });
                self.f = (self.f & C)
                    | flag(result == 0, Z)
                    | flag(subtract, N)
                    | flag(value & 15 == if subtract { 0 } else { 15 }, H);
                self.set_r(y, result, bus);
            }
            6 => {
                let value = self.immediate(bus);
                self.set_r(y, value, bus);
            }
            _ => match y {
                0..=3 => {
                    self.a = self.rotate(y, self.a);
                    self.f &= !Z;
                }
                4 => self.daa(),
                5 => {
                    self.a = !self.a;
                    self.f |= N | H;
                }
                6 => self.f = (self.f & Z) | C,
                _ => self.f = (self.f & (Z | C)) ^ C,
            },
        }
    }

    fn group_three(&mut self, y: u8, z: u8, p: u8, q: u8, bus: &mut Bus) {
        match z {
            0 => match y {
                0..=3 => {
                    bus.idle();
                    if self.condition(y) {
                        self.return_address(bus);
                    }
                }
                4 | 6 => {
                    let address = 0xff00 | u16::from(self.immediate(bus));
                    if y == 4 {
                        bus.cpu_write(address, self.a);
                    } else {
                        self.a = bus.cpu_read(address);
                    }
                }
                5 | 7 => {
                    let offset = self.immediate(bus);
                    let result = self.add_sp_offset(offset);
                    bus.idle();
                    if y == 5 {
                        bus.idle();
                        self.sp = result;
                    } else {
                        self.set_hl(result);
                    }
                }
                _ => unreachable!(),
            },
            1 if q == 0 => {
                let value = self.pop(bus);
                if p == 3 {
                    [self.a, self.f] = (value & 0xfff0).to_be_bytes();
                } else {
                    self.set_rp(p, value);
                }
            }
            1 => match p {
                0 => self.return_address(bus),
                1 => {
                    self.return_address(bus);
                    self.ime = true;
                }
                2 => self.pc = self.hl(),
                _ => {
                    bus.idle();
                    self.sp = self.hl();
                }
            },
            2 => {
                if y < 4 {
                    let address = self.immediate16(bus);
                    if self.condition(y) {
                        bus.idle();
                        self.pc = address;
                    }
                } else {
                    let address = if y & 1 == 0 {
                        0xff00 | u16::from(self.c)
                    } else {
                        self.immediate16(bus)
                    };
                    if y < 6 {
                        bus.cpu_write(address, self.a);
                    } else {
                        self.a = bus.cpu_read(address);
                    }
                }
            }
            3 => match y {
                0 => {
                    let address = self.immediate16(bus);
                    bus.idle();
                    self.pc = address;
                }
                1 => self.cb(bus),
                6 => self.ime = false,
                7 => self.ei_pending = !self.ime,
                _ => self.locked = true,
            },
            4 if y < 4 => {
                let address = self.immediate16(bus);
                if self.condition(y) {
                    bus.idle();
                    self.push(bus, self.pc);
                    self.pc = address;
                }
            }
            5 if q == 0 => {
                bus.idle();
                self.push(bus, if p == 3 { self.af() } else { self.rp(p) });
            }
            5 if p == 0 => {
                let address = self.immediate16(bus);
                bus.idle();
                self.push(bus, self.pc);
                self.pc = address;
            }
            6 => {
                let value = self.immediate(bus);
                self.alu(y, value);
            }
            7 => {
                bus.idle();
                self.push(bus, self.pc);
                self.pc = u16::from(y) * 8;
            }
            _ => self.locked = true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bcd(value: u8) -> u8 {
        ((value / 10) << 4) | (value % 10)
    }

    #[test]
    fn daa_adjusts_all_packed_decimal_additions() {
        let mut cpu = Cpu::default();
        for left in 0..100 {
            for right in 0..100 {
                cpu.a = bcd(left);
                cpu.f = 0;
                cpu.alu(0, bcd(right));
                cpu.daa();
                let expected = (left + right) % 100;
                assert_eq!(cpu.a, bcd(expected));
                assert_eq!(cpu.f, flag(expected == 0, Z) | flag(left + right >= 100, C));
            }
        }
    }

    #[test]
    fn daa_adjusts_all_packed_decimal_subtractions() {
        let mut cpu = Cpu::default();
        for left in 0..100 {
            for right in 0..100 {
                cpu.a = bcd(left);
                cpu.f = 0;
                cpu.alu(2, bcd(right));
                cpu.daa();
                let expected = (left + 100 - right) % 100;
                assert_eq!(cpu.a, bcd(expected));
                assert_eq!(cpu.f, N | flag(expected == 0, Z) | flag(left < right, C));
            }
        }
    }
}
