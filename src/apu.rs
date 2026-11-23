//! Four DMG voices, DIV-driven frame sequencer and stereo PCM at 32768 Hz.

pub const SAMPLE_RATE: u32 = 32_768;
const BUFFER_LIMIT: usize = 16_384;

const MASKS: [u8; 23] = [
    0x80, 0x3f, 0, 0xff, 0xbf, 0xff, 0x3f, 0, 0xff, 0xbf, 0x7f, 0xff, 0x9f, 0xff, 0xbf, 0xff, 0xff,
    0, 0, 0xbf, 0, 0, 0x70,
];
const DUTY: [u8; 4] = [0b0000_0001, 0b1000_0001, 0b1000_0111, 0b0111_1110];

#[derive(Debug, Default)]
struct Envelope {
    volume: u8,
    timer: u8,
}

impl Envelope {
    fn trigger(&mut self, register: u8) {
        self.volume = register >> 4;
        self.timer = Self::period(register);
    }

    fn period(register: u8) -> u8 {
        match register & 7 {
            0 => 8,
            period => period,
        }
    }

    fn clock(&mut self, register: u8) {
        if self.timer > 0 {
            self.timer -= 1;
        }
        if self.timer != 0 {
            return;
        }
        self.timer = Self::period(register);
        if register & 7 == 0 {
            return;
        }
        if register & 8 != 0 {
            self.volume = (self.volume + 1).min(15);
        } else {
            self.volume = self.volume.saturating_sub(1);
        }
    }
}

#[derive(Debug, Default)]
struct Voice {
    enabled: bool,
    length: u16,
    timer: u32,
    phase: u8,
    envelope: Envelope,
}

#[derive(Debug, Default)]
struct Sweep {
    shadow: u16,
    timer: u8,
    enabled: bool,
    negated: bool,
}

#[derive(Debug)]
pub struct Apu {
    regs: [u8; 23],
    wave_ram: [u8; 16],
    voices: [Voice; 4],
    sweep: Sweep,
    phase: u8,
    lfsr: u16,
    wave_sample: u8,
    wave_access: u8,
    sample_phase: u8,
    capacitor: [f32; 2],
    samples: Vec<i16>,
}

impl Default for Apu {
    fn default() -> Self {
        let mut regs = [0; 23];
        regs[1] = 0x80;
        regs[2] = 0xf3;
        regs[0x14] = 0x77;
        regs[0x15] = 0xf3;
        regs[0x16] = 0x80;
        let mut voices: [Voice; 4] = std::array::from_fn(|_| Voice::default());
        voices[0].enabled = true;
        voices[0].timer = 8192;
        Self {
            regs,
            wave_ram: [0; 16],
            voices,
            sweep: Sweep::default(),
            phase: 0,
            lfsr: 0x7fff,
            wave_sample: 0,
            wave_access: 0,
            sample_phase: 0,
            capacitor: [0.; 2],
            samples: Vec::with_capacity(2048),
        }
    }
}

impl Apu {
    fn powered(&self) -> bool {
        self.regs[0x16] & 0x80 != 0
    }

    fn wave_index(&self, address: u16) -> Option<usize> {
        if !self.voices[2].enabled {
            return Some(usize::from(address & 15));
        }
        (self.wave_access != 0).then_some(usize::from(self.voices[2].phase / 2))
    }

    pub fn read(&self, address: u16) -> u8 {
        match address {
            0xff26 => self
                .voices
                .iter()
                .enumerate()
                .fold(self.regs[0x16] | 0x70, |value, (i, voice)| {
                    value | (u8::from(voice.enabled) << i)
                }),
            0xff10..=0xff25 => {
                let index = usize::from(address - 0xff10);
                self.regs[index] | MASKS[index]
            }
            0xff30..=0xff3f => self.wave_index(address).map_or(0xff, |i| self.wave_ram[i]),
            _ => 0xff,
        }
    }

    fn dac(&self, channel: usize) -> bool {
        match channel {
            0 => self.regs[2] & 0xf8 != 0,
            1 => self.regs[7] & 0xf8 != 0,
            2 => self.regs[10] & 0x80 != 0,
            _ => self.regs[0x11] & 0xf8 != 0,
        }
    }

    fn frequency(&self, channel: usize) -> u16 {
        let base = 5 * channel;
        u16::from(self.regs[base + 3]) | (u16::from(self.regs[base + 4] & 7) << 8)
    }

    fn period(&self, channel: usize) -> u32 {
        if channel == 3 {
            let noise = self.regs[0x12];
            let base = if noise & 7 == 0 {
                8
            } else {
                16 * u32::from(noise & 7)
            };
            base << (noise >> 4)
        } else {
            u32::from(2048 - self.frequency(channel)) * if channel == 2 { 2 } else { 4 }
        }
    }

    fn sweep_result(&mut self) -> Option<u16> {
        let delta = self.sweep.shadow >> (self.regs[0] & 7);
        let value = if self.regs[0] & 8 != 0 {
            self.sweep.negated = true;
            self.sweep.shadow - delta
        } else {
            self.sweep.shadow + delta
        };
        if value > 2047 {
            self.voices[0].enabled = false;
            None
        } else {
            Some(value)
        }
    }

    fn sweep_period(&self) -> u8 {
        match (self.regs[0] >> 4) & 7 {
            0 => 8,
            period => period,
        }
    }

    fn clock_sweep(&mut self) {
        if self.sweep.timer > 0 {
            self.sweep.timer -= 1;
        }
        if self.sweep.timer != 0 {
            return;
        }
        self.sweep.timer = self.sweep_period();
        if self.sweep.enabled && self.regs[0] & 0x70 != 0 && self.regs[0] & 7 != 0 {
            if let Some(value) = self.sweep_result() {
                self.sweep.shadow = value;
                self.regs[3] = value as u8;
                self.regs[4] = (self.regs[4] & !7) | (value >> 8) as u8;
                self.sweep_result();
            }
        }
    }

    fn trigger(&mut self, channel: usize, length_enabled: bool) {
        let dac = self.dac(channel);
        let period = self.period(channel);
        let voice = &mut self.voices[channel];
        voice.enabled = dac;
        if voice.length == 0 {
            voice.length = if channel == 2 { 256 } else { 64 };
            if length_enabled && self.phase & 1 != 0 {
                voice.length -= 1;
            }
        }
        voice.timer = period;
        match channel {
            0 | 1 => voice.envelope.trigger(self.regs[channel * 5 + 2]),
            2 => {
                voice.phase = 0;
                self.wave_access = 0;
            }
            _ => {
                voice.envelope.trigger(self.regs[0x11]);
                self.lfsr = 0x7fff;
            }
        }
        if channel == 0 {
            self.sweep = Sweep {
                shadow: self.frequency(0),
                timer: self.sweep_period(),
                enabled: self.regs[0] & 0x77 != 0,
                negated: false,
            };
            if self.regs[0] & 7 != 0 {
                self.sweep_result();
            }
        }
    }

    pub fn write(&mut self, address: u16, value: u8) {
        if (0xff30..=0xff3f).contains(&address) {
            if let Some(index) = self.wave_index(address) {
                self.wave_ram[index] = value;
            }
            return;
        }
        if !(0xff10..=0xff26).contains(&address) {
            return;
        }
        let index = usize::from(address - 0xff10);
        if index == 0x16 {
            if value & 0x80 == 0 {
                self.regs.fill(0);
                self.sweep = Sweep::default();
                for voice in &mut self.voices {
                    *voice = Voice {
                        length: voice.length,
                        ..Voice::default()
                    };
                }
            } else if !self.powered() {
                self.phase = 0;
                self.wave_sample = 0;
                self.regs[index] = 0x80;
            }
            return;
        }

        let length = match index {
            1 => Some(0),
            6 => Some(1),
            11 => Some(2),
            16 => Some(3),
            _ => None,
        };
        if let Some(channel) = length {
            self.voices[channel].length = if channel == 2 {
                256 - u16::from(value)
            } else {
                64 - u16::from(value & 63)
            };
        }
        if !self.powered() {
            return;
        }

        let previous = std::mem::replace(&mut self.regs[index], value);
        match index {
            0 if self.sweep.negated && value & 8 == 0 => self.voices[0].enabled = false,
            2 if !self.dac(0) => self.voices[0].enabled = false,
            7 if !self.dac(1) => self.voices[1].enabled = false,
            10 if !self.dac(2) => self.voices[2].enabled = false,
            17 if !self.dac(3) => self.voices[3].enabled = false,
            4 | 9 | 14 | 19 => {
                let channel = index / 5;
                let length_enabled = value & 0x40 != 0;
                let voice = &mut self.voices[channel];
                if length_enabled && previous & 0x40 == 0 && self.phase & 1 != 0 && voice.length > 0
                {
                    voice.length -= 1;
                    if voice.length == 0 {
                        voice.enabled = false;
                    }
                }
                if value & 0x80 != 0 {
                    self.trigger(channel, length_enabled);
                }
            }
            _ => {}
        }
    }

    pub fn clock_frame(&mut self) {
        if !self.powered() {
            return;
        }
        if self.phase & 1 == 0 {
            for (i, voice) in self.voices.iter_mut().enumerate() {
                if self.regs[i * 5 + 4] & 0x40 != 0 && voice.length != 0 {
                    voice.length -= 1;
                    if voice.length == 0 {
                        voice.enabled = false;
                    }
                }
            }
        }
        if matches!(self.phase, 2 | 6) {
            self.clock_sweep();
        }
        if self.phase == 7 {
            for (channel, register) in [(0, 2), (1, 7), (3, 17)] {
                self.voices[channel].envelope.clock(self.regs[register]);
            }
        }
        self.phase = (self.phase + 1) & 7;
    }

    pub fn tick(&mut self) {
        self.wave_access = self.wave_access.saturating_sub(1);
        if self.powered() {
            for i in 0..4 {
                let voice = &mut self.voices[i];
                if !voice.enabled {
                    continue;
                }
                voice.timer -= 1;
                if voice.timer != 0 {
                    continue;
                }
                match i {
                    0 | 1 => voice.phase = (voice.phase + 1) & 7,
                    2 => {
                        voice.phase = (voice.phase + 1) & 31;
                        self.wave_sample = self.wave_ram[usize::from(voice.phase / 2)];
                        self.wave_access = 2;
                    }
                    _ => {
                        let bit = (self.lfsr ^ (self.lfsr >> 1)) & 1;
                        self.lfsr = (self.lfsr >> 1) | (bit << 14);
                        if self.regs[0x12] & 8 != 0 {
                            self.lfsr = (self.lfsr & !(1 << 6)) | (bit << 6);
                        }
                    }
                }
                self.voices[i].timer = self.period(i);
            }
        }
        self.sample_phase = (self.sample_phase + 1) & 127;
        if self.sample_phase == 0 && self.samples.len() < BUFFER_LIMIT {
            self.mix();
        }
    }

    fn digital(&self, i: usize) -> u8 {
        let voice = &self.voices[i];
        if !voice.enabled {
            return 0;
        }
        match i {
            0 | 1 => {
                let duty = DUTY[usize::from(self.regs[i * 5 + 1] >> 6)];
                if duty & (0x80 >> voice.phase) == 0 {
                    0
                } else {
                    voice.envelope.volume
                }
            }
            2 => {
                let level = (self.regs[0x0c] >> 5) & 3;
                let nibble = if voice.phase & 1 == 0 {
                    self.wave_sample >> 4
                } else {
                    self.wave_sample & 15
                };
                if level == 0 {
                    0
                } else {
                    nibble >> (level - 1)
                }
            }
            _ => {
                if self.lfsr & 1 == 0 {
                    voice.envelope.volume
                } else {
                    0
                }
            }
        }
    }

    fn mix(&mut self) {
        let mut stereo = [0i16; 2];
        for (side, sample) in stereo.iter_mut().enumerate() {
            let shift = 4 * (1 - side);
            let route = self.regs[0x15] >> shift;
            let volume = (self.regs[0x14] >> shift & 7) + 1;
            let mut sum = 0;
            for i in 0..4 {
                if route & (1 << i) != 0 && self.dac(i) {
                    sum += i32::from(self.digital(i)) * 2 - 15;
                }
            }
            let input = (sum * i32::from(volume)) as f32 / 480.;
            let output = input - self.capacitor[side];
            self.capacitor[side] = input - output * 0.995;
            *sample = (output * 24_000.).clamp(-32768., 32767.) as i16;
        }
        self.samples.extend_from_slice(&stereo);
    }

    pub fn samples(&self) -> &[i16] {
        &self.samples
    }
    pub fn clear_samples(&mut self) {
        self.samples.clear();
    }
}
