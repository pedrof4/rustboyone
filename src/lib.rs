//! Game Boy DMG emulation core shared by the CLI and browser.

pub mod apu;
pub mod bus;
pub mod cartridge;
pub mod cpu;
pub mod ppu;
pub mod timer;

#[cfg(target_arch = "wasm32")]
mod wasm;

use bus::Bus;
use cartridge::{Cartridge, CartridgeError};
use cpu::Cpu;

pub const CLOCK_HZ: u32 = 4_194_304;
pub const FRAME_T_STATES: u32 = 70_224;

#[derive(Debug)]
pub struct GameBoy {
    pub cpu: Cpu,
    pub bus: Bus,
}

impl GameBoy {
    pub fn new(rom: Vec<u8>) -> Result<Self, CartridgeError> {
        Ok(Self {
            cpu: Cpu::default(),
            bus: Bus::new(Cartridge::new(rom)?),
        })
    }

    /// Executes an instruction, interrupt entry or one idle machine cycle.
    pub fn step(&mut self) -> u8 {
        let start = self.bus.cycles();
        self.cpu.step(&mut self.bus);
        (self.bus.cycles() - start) as u8
    }

    /// Run for the given budget, rounding up at the final instruction boundary.
    pub fn run_cycles(&mut self, cycles: u64) -> u64 {
        let start = self.bus.cycles();
        while self.bus.cycles() - start < cycles {
            self.step();
        }
        self.bus.cycles() - start
    }
}
