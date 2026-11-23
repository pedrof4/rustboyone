//! Lightweight WebAssembly ABI. The host copies into a Rust-owned staging buffer
//! and reads immutable snapshots before the next call into the emulator.

use crate::GameBoy;
use std::cell::RefCell;

#[derive(Default)]
struct Browser {
    game: Option<GameBoy>,
    staging: Vec<u8>,
    error: String,
}

thread_local! {
    static BROWSER: RefCell<Browser> = RefCell::default();
}

#[no_mangle]
extern "C" fn dmg_staging(length: usize) -> *mut u8 {
    if length > 8 * 1024 * 1024 {
        return std::ptr::null_mut();
    }
    BROWSER.with_borrow_mut(|browser| {
        browser.staging.resize(length, 0);
        browser.staging.as_mut_ptr()
    })
}

#[no_mangle]
extern "C" fn dmg_load() -> u32 {
    BROWSER.with_borrow_mut(
        |browser| match GameBoy::new(std::mem::take(&mut browser.staging)) {
            Ok(game) => {
                browser.game = Some(game);
                browser.error.clear();
                1
            }
            Err(error) => {
                browser.error = error.to_string();
                0
            }
        },
    )
}

#[no_mangle]
extern "C" fn dmg_error_ptr() -> *const u8 {
    BROWSER.with_borrow(|browser| browser.error.as_ptr())
}

#[no_mangle]
extern "C" fn dmg_error_len() -> usize {
    BROWSER.with_borrow(|browser| browser.error.len())
}

#[no_mangle]
extern "C" fn dmg_run(cycles: u32, keys: u8) -> i32 {
    BROWSER.with_borrow_mut(|browser| {
        let Some(game) = &mut browser.game else {
            return 0;
        };
        game.bus.set_keys(keys);
        let start = game.bus.cycles();
        let budget = u64::from(cycles.min(2 * crate::FRAME_T_STATES));
        while !game.cpu.locked && game.bus.cycles() - start < budget {
            game.step();
        }
        if game.cpu.locked {
            -1
        } else {
            (game.bus.cycles() - start) as i32
        }
    })
}

#[no_mangle]
extern "C" fn dmg_frame() -> *const u8 {
    BROWSER.with_borrow(|browser| {
        browser
            .game
            .as_ref()
            .map_or(std::ptr::null(), |game| game.bus.ppu.framebuffer().as_ptr())
    })
}

#[no_mangle]
extern "C" fn dmg_audio_ptr() -> *const i16 {
    BROWSER.with_borrow(|browser| {
        browser
            .game
            .as_ref()
            .map_or(std::ptr::null(), |game| game.bus.apu.samples().as_ptr())
    })
}

#[no_mangle]
extern "C" fn dmg_audio_len() -> usize {
    BROWSER.with_borrow(|browser| {
        browser
            .game
            .as_ref()
            .map_or(0, |game| game.bus.apu.samples().len())
    })
}

#[no_mangle]
extern "C" fn dmg_audio_clear() {
    BROWSER.with_borrow_mut(|browser| {
        if let Some(game) = &mut browser.game {
            game.bus.apu.clear_samples();
            game.bus.clear_serial();
        }
    });
}

#[no_mangle]
extern "C" fn dmg_audio_rate() -> u32 {
    crate::apu::SAMPLE_RATE
}

#[no_mangle]
extern "C" fn dmg_clock() -> u32 {
    crate::CLOCK_HZ
}

#[no_mangle]
extern "C" fn dmg_save_ptr() -> *const u8 {
    BROWSER.with_borrow(|browser| {
        browser
            .game
            .as_ref()
            .and_then(|game| game.bus.cartridge.battery_ram())
            .map_or(std::ptr::null(), |ram| ram.as_ptr())
    })
}

#[no_mangle]
extern "C" fn dmg_save_len() -> usize {
    BROWSER.with_borrow(|browser| {
        browser
            .game
            .as_ref()
            .and_then(|game| game.bus.cartridge.battery_ram())
            .map_or(0, <[u8]>::len)
    })
}

#[no_mangle]
extern "C" fn dmg_restore() {
    BROWSER.with_borrow_mut(|browser| {
        if let Some(game) = &mut browser.game {
            game.bus.cartridge.restore_battery_ram(&browser.staging);
        }
        browser.staging.clear();
    });
}
