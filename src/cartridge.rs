use std::{error::Error, fmt};

const ROM_BANK: usize = 0x4000;
const RAM_BANK: usize = 0x2000;
const CLOCK: u32 = 4_194_304;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CartridgeError {
    HeaderTooShort,
    UnsupportedMapper(u8),
    InvalidRomSize(u8),
    InvalidRamSize(u8),
    TruncatedRom { expected: usize, actual: usize },
}

impl fmt::Display for CartridgeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::HeaderTooShort => write!(f, "cabeçalho do cartucho incompleto"),
            Self::UnsupportedMapper(x) => write!(f, "mapeador 0x{x:02X} não suportado"),
            Self::InvalidRomSize(x) => write!(f, "código de tamanho de ROM inválido: 0x{x:02X}"),
            Self::InvalidRamSize(x) => write!(f, "código de tamanho de RAM inválido: 0x{x:02X}"),
            Self::TruncatedRom { expected, actual } => {
                write!(f, "ROM truncada: {actual} de {expected} bytes")
            }
        }
    }
}

impl Error for CartridgeError {}

#[derive(Debug)]
enum Mapper {
    Rom,
    Mbc1 {
        low: u8,
        high: u8,
        mode: bool,
        multicart: bool,
    },
    Mbc2 {
        bank: u8,
    },
    Mbc3 {
        bank: u8,
        select: u8,
    },
    Mbc5 {
        bank: u16,
        ram: u8,
        rumble: bool,
    },
}

#[derive(Debug, Default)]
struct Rtc {
    live: [u8; 5],
    latched: [u8; 5],
    has_latch: bool,
    latch_input: u8,
    clocks: u32,
}

impl Rtc {
    fn tick(&mut self, cycles: u32) {
        if self.live[4] & 0x40 != 0 {
            return;
        }
        let clocks = u64::from(self.clocks) + u64::from(cycles);
        self.clocks = (clocks % u64::from(CLOCK)) as u32;
        for _ in 0..clocks / u64::from(CLOCK) {
            self.second();
        }
    }

    fn second(&mut self) {
        for (i, limit) in [60, 60, 24].into_iter().enumerate() {
            self.live[i] = self.live[i].wrapping_add(1) & [63, 63, 31][i];
            if self.live[i] != limit {
                return;
            }
            self.live[i] = 0;
        }
        let (days, carry) = self.live[3].overflowing_add(1);
        self.live[3] = days;
        if carry {
            self.live[4] ^= 1;
            if self.live[4] & 1 == 0 {
                self.live[4] |= 0x80;
            }
        }
    }

    fn latch(&mut self, value: u8) {
        if self.latch_input == 0 && value == 1 {
            self.latched = self.live;
            self.has_latch = true;
        }
        self.latch_input = value;
    }

    fn read(&self, index: usize) -> u8 {
        if self.has_latch {
            self.latched[index]
        } else {
            self.live[index]
        }
    }

    fn write(&mut self, index: usize, value: u8) {
        self.live[index] = value & [0x3f, 0x3f, 0x1f, 0xff, 0xc1][index];
        if index == 0 {
            self.clocks = 0;
        }
    }
}

#[derive(Debug)]
pub struct Cartridge {
    rom: Vec<u8>,
    ram: Vec<u8>,
    mapper: Mapper,
    ram_enabled: bool,
    battery: bool,
    rtc: Option<Rtc>,
}

impl Cartridge {
    pub fn new(mut rom: Vec<u8>) -> Result<Self, CartridgeError> {
        if rom.len() < 0x150 {
            return Err(CartridgeError::HeaderTooShort);
        }
        let kind = rom[0x147];
        let banks = match rom[0x148] {
            n @ 0..=8 => 2 << n,
            0x52 => 72,
            0x53 => 80,
            0x54 => 96,
            n => return Err(CartridgeError::InvalidRomSize(n)),
        };
        let expected = banks * ROM_BANK;
        if rom.len() < expected {
            return Err(CartridgeError::TruncatedRom {
                expected,
                actual: rom.len(),
            });
        }
        rom.truncate(expected);
        let mapper = match kind {
            0 | 8 | 9 => Mapper::Rom,
            1..=3 => {
                let logo = &rom[0x104..0x134];
                let multicart = banks == 64
                    && logo.iter().any(|&byte| byte != 0)
                    && rom[0x40104..0x40134] == *logo;
                Mapper::Mbc1 {
                    low: 1,
                    high: 0,
                    mode: false,
                    multicart,
                }
            }
            5 | 6 => Mapper::Mbc2 { bank: 1 },
            0x0f..=0x13 => Mapper::Mbc3 { bank: 1, select: 0 },
            0x19..=0x1e => Mapper::Mbc5 {
                bank: 1,
                ram: 0,
                rumble: kind >= 0x1c,
            },
            _ => return Err(CartridgeError::UnsupportedMapper(kind)),
        };
        let ram_size = match kind {
            5 | 6 => 512,
            2 | 3 | 8 | 9 | 0x10 | 0x12 | 0x13 | 0x1a | 0x1b | 0x1d | 0x1e => match rom[0x149] {
                0 => 0,
                1 => 0x800,
                2 => RAM_BANK,
                3 => 4 * RAM_BANK,
                4 => 16 * RAM_BANK,
                5 => 8 * RAM_BANK,
                n => return Err(CartridgeError::InvalidRamSize(n)),
            },
            _ => 0,
        };
        Ok(Self {
            rom,
            ram: vec![0xff; ram_size],
            mapper,
            ram_enabled: matches!(kind, 0 | 8 | 9),
            battery: matches!(kind, 3 | 6 | 9 | 0x0f | 0x10 | 0x13 | 0x1b | 0x1e),
            rtc: matches!(kind, 0x0f | 0x10).then(Rtc::default),
        })
    }

    pub fn battery_ram(&self) -> Option<&[u8]> {
        self.battery.then_some(self.ram.as_slice())
    }

    pub fn restore_battery_ram(&mut self, bytes: &[u8]) -> bool {
        if self.battery && bytes.len() == self.ram.len() {
            self.ram.copy_from_slice(bytes);
            true
        } else {
            false
        }
    }

    pub fn tick(&mut self, cycles: u32) {
        if let Some(rtc) = &mut self.rtc {
            rtc.tick(cycles);
        }
    }

    fn rom_bank(&self, upper: bool) -> usize {
        match (&self.mapper, upper) {
            (Mapper::Rom, true) => 1,
            (
                Mapper::Mbc1 {
                    low,
                    high,
                    multicart,
                    ..
                },
                true,
            ) => {
                let shift = if *multicart { 4 } else { 5 };
                (usize::from(*high) << shift) | usize::from(*low & ((1 << shift) - 1))
            }
            (
                Mapper::Mbc1 {
                    high,
                    mode: true,
                    multicart,
                    ..
                },
                false,
            ) => usize::from(*high) << if *multicart { 4 } else { 5 },
            (Mapper::Mbc2 { bank } | Mapper::Mbc3 { bank, .. }, true) => usize::from(*bank),
            (Mapper::Mbc5 { bank, .. }, true) => usize::from(*bank),
            _ => 0,
        }
    }

    fn ram_index(&self, address: u16) -> Option<usize> {
        let bank = match self.mapper {
            Mapper::Mbc2 { .. } => return Some(usize::from(address & 0x01ff)),
            Mapper::Mbc1 {
                high, mode: true, ..
            } => high,
            Mapper::Mbc3 { select, .. } if select <= 3 => select,
            Mapper::Mbc3 { .. } => return None,
            Mapper::Mbc5 { ram, .. } => ram,
            _ => 0,
        };
        if self.ram.is_empty() {
            return None;
        }
        Some((usize::from(bank) * RAM_BANK + usize::from(address & 0x1fff)) % self.ram.len())
    }

    pub fn read(&self, address: u16) -> u8 {
        match address {
            0x0000..=0x7fff => {
                let bank = self.rom_bank(address >= 0x4000) % (self.rom.len() / ROM_BANK);
                self.rom[bank * ROM_BANK + usize::from(address & 0x3fff)]
            }
            0xa000..=0xbfff if self.ram_enabled => {
                if let Mapper::Mbc3 {
                    select: select @ 8..=12,
                    ..
                } = self.mapper
                {
                    return self
                        .rtc
                        .as_ref()
                        .map_or(0xff, |rtc| rtc.read(usize::from(select - 8)));
                }
                self.ram_index(address).map_or(0xff, |index| {
                    self.ram[index]
                        | if matches!(self.mapper, Mapper::Mbc2 { .. }) {
                            0xf0
                        } else {
                            0
                        }
                })
            }
            _ => 0xff,
        }
    }

    pub fn write(&mut self, address: u16, value: u8) {
        if (0xa000..=0xbfff).contains(&address) && self.ram_enabled {
            if let Mapper::Mbc3 {
                select: select @ 8..=12,
                ..
            } = self.mapper
            {
                if let Some(rtc) = &mut self.rtc {
                    rtc.write(usize::from(select - 8), value);
                }
            } else if let Some(index) = self.ram_index(address) {
                self.ram[index] = if matches!(self.mapper, Mapper::Mbc2 { .. }) {
                    value & 15
                } else {
                    value
                };
            }
            return;
        }
        match (&mut self.mapper, address) {
            (Mapper::Rom, _) => {}
            (Mapper::Mbc2 { .. }, 0..=0x3fff) if address & 0x100 == 0 => {
                self.ram_enabled = value & 15 == 10;
            }
            (Mapper::Mbc2 { bank }, 0..=0x3fff) => *bank = (value & 15).max(1),
            (Mapper::Mbc2 { .. }, _) => {}
            (_, 0..=0x1fff) => self.ram_enabled = value & 15 == 10,
            (Mapper::Mbc1 { low, .. }, 0x2000..=0x3fff) => *low = (value & 31).max(1),
            (Mapper::Mbc1 { high, .. }, 0x4000..=0x5fff) => *high = value & 3,
            (Mapper::Mbc1 { mode, .. }, 0x6000..=0x7fff) => *mode = value & 1 != 0,
            (Mapper::Mbc3 { bank, .. }, 0x2000..=0x3fff) => *bank = (value & 127).max(1),
            (Mapper::Mbc3 { select, .. }, 0x4000..=0x5fff) => *select = value,
            (Mapper::Mbc3 { .. }, 0x6000..=0x7fff) => {
                if let Some(rtc) = &mut self.rtc {
                    rtc.latch(value);
                }
            }
            (Mapper::Mbc5 { bank, .. }, 0x2000..=0x2fff) => {
                *bank = (*bank & 0x100) | u16::from(value)
            }
            (Mapper::Mbc5 { bank, .. }, 0x3000..=0x3fff) => {
                *bank = (*bank & 0xff) | (u16::from(value & 1) << 8)
            }
            (Mapper::Mbc5 { ram, rumble, .. }, 0x4000..=0x5fff) => {
                *ram = value & if *rumble { 7 } else { 15 }
            }
            _ => {}
        }
    }
}
