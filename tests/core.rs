use dmg_emu::{
    bus::Bus,
    cartridge::Cartridge,
    ppu::{HEIGHT, WIDTH},
    timer::Timer,
    GameBoy,
};

fn rom(kind: u8, size: u8, ram: u8) -> Vec<u8> {
    let mut bytes = vec![0; 0x8000 << size];
    bytes[0x147] = kind;
    bytes[0x148] = size;
    bytes[0x149] = ram;
    for bank in 0..bytes.len() / 0x4000 {
        let offset = bank * 0x4000 + 0x200;
        bytes[offset..offset + 2].copy_from_slice(&(bank as u16).to_le_bytes());
    }
    bytes
}

fn program(code: &[u8]) -> GameBoy {
    let mut bytes = rom(0, 0, 0);
    bytes[0x100..0x100 + code.len()].copy_from_slice(code);
    GameBoy::new(bytes).unwrap()
}

#[test]
fn decode_all_base_and_cb_opcodes() {
    let invalid = [
        0xd3, 0xdb, 0xdd, 0xe3, 0xe4, 0xeb, 0xec, 0xed, 0xf4, 0xfc, 0xfd,
    ];
    for opcode in 0..=255 {
        let mut gb = program(&[opcode, 0, 0]);
        let cycles = gb.step();
        assert!((4..=24).contains(&cycles), "opcode {opcode:02X}");
        assert_eq!(
            gb.cpu.locked,
            invalid.contains(&opcode),
            "opcode {opcode:02X}"
        );
        assert_eq!(gb.cpu.f & 15, 0, "opcode {opcode:02X}");
    }
    for opcode in 0..=255 {
        let mut gb = program(&[0xcb, opcode]);
        let cycles = match (opcode & 7, opcode >> 6) {
            (6, 1) => 12,
            (6, _) => 16,
            _ => 8,
        };
        assert_eq!(gb.step(), cycles, "CB {opcode:02X}");
        assert_eq!(gb.cpu.pc, 0x102);
        assert_eq!(gb.cpu.f & 15, 0);
    }
}

#[test]
fn arithmetic_flags_and_signed_stack_offset() {
    let mut gb = program(&[
        0x3e, 0xff, // LD A, FF
        0xc6, 0x01, // ADD A, 01
        0xce, 0x0f, // ADC A, 0F
        0xd6, 0x01, // SUB 01
        0x37, // SCF
        0xde, 0x0e, // SBC A, 0E
        0xfe, 0x00, // CP 00
        0xa7, // AND A
        0xaf, // XOR A
        0xe8, 0xff, // ADD SP, -1
        0xf8, 0x03, // LD HL, SP+3
    ]);
    for expected in [
        0xffb0, 0x00b0, 0x1020, 0x0f60, 0x0f10, 0x00c0, 0x00c0, 0x00a0, 0x0080,
    ] {
        gb.step();
        assert_eq!(gb.cpu.af(), expected);
    }
    assert_eq!(gb.step(), 16);
    assert_eq!((gb.cpu.sp, gb.cpu.f), (0xfffd, 0x30));
    assert_eq!(gb.step(), 12);
    assert_eq!((gb.cpu.hl(), gb.cpu.f), (0, 0x30));
}

#[test]
fn cb_memory_bit_test_and_bit_writes() {
    let mut gb = program(&[
        0x21, 0, 0xc0, 0x36, 0x80, 0xcb, 0x06, 0xcb, 0x7e, 0xcb, 0xfe, 0xcb, 0x86,
    ]);
    for cycles in [12, 12, 16, 12] {
        assert_eq!(gb.step(), cycles);
    }
    assert_eq!(gb.cpu.f, 0xb0); // BIT preserves carry, sets H and Z.
    assert_eq!(gb.bus.peek(0xc000), 1);
    assert_eq!(gb.step(), 16);
    assert_eq!(gb.bus.peek(0xc000), 0x81);
    assert_eq!(gb.step(), 16);
    assert_eq!(gb.bus.peek(0xc000), 0x80);
    assert_eq!(gb.cpu.f, 0xb0);
}

#[test]
fn conditional_control_and_stack_order() {
    let mut gb = program(&[0xaf, 0xc4, 0x0a, 1, 0xcc, 0x0a, 1, 0, 0, 0, 0xc8]);
    assert_eq!(gb.step(), 4);
    assert_eq!(gb.step(), 12);
    assert_eq!(gb.step(), 24);
    assert_eq!((gb.cpu.pc, gb.cpu.sp), (0x10a, 0xfffc));
    assert_eq!((gb.bus.peek(0xfffc), gb.bus.peek(0xfffd)), (7, 1));
    assert_eq!(gb.step(), 20);
    assert_eq!((gb.cpu.pc, gb.cpu.sp), (0x107, 0xfffe));
}

#[test]
fn delayed_ei_interrupt_priority_and_halt_bug() {
    let mut gb = program(&[0xfb, 0x00, 0x00]);
    gb.bus.cpu_write(0xffff, 0x1f);
    gb.bus.cpu_write(0xff0f, 0x1f);
    assert_eq!(gb.step(), 4);
    assert!(!gb.cpu.ime);
    assert_eq!(gb.step(), 4);
    assert!(gb.cpu.ime);
    assert_eq!(gb.step(), 20);
    assert_eq!(gb.cpu.pc, 0x40);
    assert_eq!(gb.bus.peek(0xff0f), 0xfe);
    assert_eq!(gb.bus.peek(0xfffc), 2);

    let mut gb = program(&[0x76, 0x3e, 0x11]);
    gb.bus.cpu_write(0xffff, 1);
    gb.bus.cpu_write(0xff0f, 1);
    assert_eq!(gb.step(), 4);
    assert!(!gb.cpu.halted);
    assert_eq!(gb.step(), 8);
    assert_eq!((gb.cpu.a, gb.cpu.pc), (0x3e, 0x102));
}

#[test]
fn stop_freezes_clock_until_joypad_edge() {
    let mut gb = program(&[0x10, 0, 0x3e, 0x42]);
    gb.bus.cpu_write(0xff00, 0x20);
    assert_eq!(gb.step(), 4);
    assert!(gb.cpu.stopped);
    let dot = gb.bus.ppu.dot();
    gb.run_cycles(2048);
    assert_eq!(gb.bus.peek(0xff04), 0);
    assert_eq!(gb.bus.ppu.dot(), dot);
    gb.bus.set_keys(1);
    assert_eq!(gb.bus.peek(0xff00), 0xee);
    assert_ne!(gb.bus.peek(0xff0f) & 0x10, 0);
    assert_eq!(gb.step(), 8);
    assert_eq!(gb.cpu.a, 0x42);
    assert!(!gb.cpu.stopped);
}

#[test]
fn timer_frequencies_falling_edges_and_reload_writes() {
    for (clock, period) in [(0, 1024), (1, 16), (2, 64), (3, 256)] {
        let mut timer = Timer::new(0);
        timer.write(0xff07, 4 | clock);
        for _ in 1..period {
            assert!(!timer.tick());
        }
        assert_eq!(timer.read(0xff05), 0);
        assert!(!timer.tick());
        assert_eq!(timer.read(0xff05), 1);
    }
    let mut timer = Timer::new(8);
    timer.write(0xff07, 5);
    timer.write(0xff05, 255);
    timer.write(0xff06, 0xcd);
    timer.write(0xff04, 0);
    assert_eq!(timer.read(0xff05), 0);
    for _ in 0..3 {
        assert!(!timer.tick());
    }
    assert!(timer.tick());
    timer.write(0xff05, 0x77);
    assert_eq!(timer.read(0xff05), 0xcd);
    timer.write(0xff06, 0xa5);
    assert_eq!(timer.read(0xff05), 0xa5);

    let mut timer = Timer::new(8);
    timer.write(0xff05, 255);
    timer.write(0xff07, 5);
    timer.write(0xff07, 0);
    timer.tick();
    timer.write(0xff05, 0x50);
    for _ in 0..4 {
        assert!(!timer.tick());
    }
    assert_eq!(timer.read(0xff05), 0x50);
}

#[test]
fn serial_internal_and_external_transfer() {
    let mut gb = program(&[]);
    gb.bus.cpu_write(0xff04, 0);
    gb.bus.cpu_write(0xff01, b'A');
    gb.bus.cpu_write(0xff02, 0x81);
    gb.run_cycles(4_096 - 16);
    assert!(gb.bus.serial().is_empty());
    gb.run_cycles(32);
    assert_eq!(gb.bus.serial(), b"A");
    assert_eq!(gb.bus.peek(0xff01), 0xff);
    assert_eq!(gb.bus.peek(0xff02), 0x7f);
    assert_eq!(gb.bus.peek(0xff0f) & 8, 8);
    gb.bus.cpu_write(0xff01, 0xc5);
    gb.bus.cpu_write(0xff02, 0x80);
    let mut outgoing = 0;
    for bit in (0..8).rev() {
        let incoming = 0xb3 & (1 << bit) != 0;
        outgoing = (outgoing << 1) | u8::from(gb.bus.serial_external_clock(incoming).unwrap());
    }
    assert_eq!(outgoing, 0xc5);
    assert_eq!(gb.bus.peek(0xff01), 0xb3);
    assert_eq!(gb.bus.serial(), &[b'A', 0xc5]);
    assert_eq!(gb.bus.serial_external_clock(true), None);
}

#[test]
fn memory_mirroring_and_dma_progress() {
    let mut bus = Bus::new(Cartridge::new(rom(0, 0, 0)).unwrap());
    bus.cpu_write(0xff40, 0);
    bus.cpu_write(0xc123, 0x32);
    assert_eq!(bus.cpu_read(0xe123), 0x32);
    bus.cpu_write(0xfdff, 0x65);
    assert_eq!(bus.cpu_read(0xddff), 0x65);
    bus.cpu_write(0xfea0, 0x33);
    assert_eq!(bus.cpu_read(0xfea0), 0);
    for offset in 0..160 {
        bus.cpu_write(0xc000 + offset, (offset + 1) as u8);
    }
    bus.cpu_write(0xff46, 0xe0); // DMG mirrors source pages E0–FF.
    assert_eq!(bus.peek(0xfe00), 0);
    bus.idle();
    assert_eq!(bus.peek(0xfe00), 1);
    assert_eq!(bus.peek(0xfe01), 0);
    assert_eq!(bus.cpu_read(0xfe00), 0xff);
    bus.cpu_write(0xff80, 0x42);
    assert_eq!(bus.cpu_read(0xff80), 0x42);
    for _ in 0..160 {
        bus.idle();
    }
    assert_eq!(bus.cpu_read(0xfe00), 1);
    assert_eq!(bus.cpu_read(0xfe9f), 160);
}

#[test]
fn cartridge_validation_and_rom_only_ram() {
    assert!(Cartridge::new(vec![0; 0x100]).is_err());
    let mut truncated = rom(0, 0, 0);
    truncated[0x148] = 3;
    assert!(Cartridge::new(truncated).is_err());
    assert!(Cartridge::new(rom(0x22, 0, 0)).is_err());
    let mut cart = Cartridge::new(rom(9, 0, 2)).unwrap();
    assert_eq!(cart.read(0x4200), 1);
    cart.write(0xa000, 0x93);
    assert_eq!(cart.read(0xa000), 0x93);
    assert!(!cart.restore_battery_ram(&[1]));
    assert_eq!(cart.battery_ram().unwrap()[0], 0x93);
}

#[test]
fn mbc1_rom_modes_and_ram_banks() {
    let mut cart = Cartridge::new(rom(3, 5, 3)).unwrap();
    cart.write(0x2000, 0);
    cart.write(0x4000, 1);
    assert_eq!(cart.read(0x4200), 33);
    assert_eq!(cart.read(0x0200), 0);
    cart.write(0x6000, 1);
    assert_eq!(cart.read(0x0200), 32);
    assert_eq!(cart.read(0xa123), 0xff);
    cart.write(0x0000, 0x5a);
    cart.write(0xa123, 0x73);
    cart.write(0x6000, 0);
    assert_eq!(cart.read(0xa123), 0xff);
    cart.write(0x6000, 1);
    assert_eq!(cart.read(0xa123), 0x73);
}

#[test]
fn mbc2_address_bit_ram_nibble_and_mirror() {
    let mut cart = Cartridge::new(rom(6, 3, 0)).unwrap();
    cart.write(0x0100, 9);
    assert_eq!(cart.read(0x4200), 9);
    cart.write(0x2000, 10);
    cart.write(0xa000, 0x73);
    assert_eq!(cart.read(0xbe00), 0xf3);
    cart.write(0x2100, 0);
    assert_eq!(cart.read(0x4200), 1);
    cart.write(0x0000, 0);
    assert_eq!(cart.read(0xa000), 0xff);
}

#[test]
fn mbc3_rtc_latch_halt_and_day_carry() {
    let mut cart = Cartridge::new(rom(0x10, 2, 3)).unwrap();
    cart.write(0x2000, 0);
    assert_eq!(cart.read(0x4200), 1);
    cart.write(0, 10);
    for (reg, value) in [59, 59, 23, 255, 1].into_iter().enumerate() {
        cart.write(0x4000, 8 + reg as u8);
        cart.write(0xa000, value);
    }
    cart.write(0x6000, 0);
    cart.write(0x6000, 1);
    cart.tick(dmg_emu::CLOCK_HZ);
    assert_eq!(cart.read(0xa000), 1);
    cart.write(0x6000, 0);
    cart.write(0x6000, 1);
    assert_eq!(cart.read(0xa000), 0x80);
    cart.write(0xa000, 0xc0);
    cart.tick(dmg_emu::CLOCK_HZ * 2);
    cart.write(0x6000, 0);
    cart.write(0x6000, 1);
    cart.write(0x4000, 8);
    assert_eq!(cart.read(0xa000), 0);
}

#[test]
fn mbc5_ninth_rom_bit_and_ram_bank() {
    let mut cart = Cartridge::new(rom(0x1b, 8, 4)).unwrap();
    cart.write(0x2000, 0);
    assert_eq!(cart.read(0x4200), 0);
    cart.write(0x3000, 1);
    assert_eq!(cart.read(0x4201), 1);
    cart.write(0, 10);
    cart.write(0x4000, 15);
    cart.write(0xa000, 0x56);
    cart.write(0x4000, 7);
    assert_eq!(cart.read(0xa000), 0xff);
    cart.write(0x4000, 15);
    assert_eq!(cart.read(0xa000), 0x56);
}

#[test]
fn ppu_modes_stat_and_line_153() {
    let mut gb = program(&[0x76]);
    gb.bus.cpu_write(0xff40, 0);
    gb.bus.cpu_write(0xff41, 0x20);
    gb.bus.cpu_write(0xff0f, 0);
    gb.bus.cpu_write(0xff40, 0x91);
    assert_eq!(gb.bus.ppu.mode(), 0);
    gb.run_cycles(76);
    assert_eq!(gb.bus.ppu.dot(), 80);
    assert_eq!(gb.bus.ppu.mode(), 3);
    gb.run_cycles(172);
    assert_eq!(gb.bus.ppu.mode(), 0);
    gb.run_cycles(204);
    assert_eq!(gb.bus.ppu.mode(), 2);
    assert_eq!(gb.bus.peek(0xff44), 1);
    assert_ne!(gb.bus.peek(0xff0f) & 2, 0);
    gb.run_cycles(456 * 143 - 4);
    assert_eq!(gb.bus.ppu.mode(), 1);
    assert_eq!(gb.bus.peek(0xff44), 144);
    assert_eq!(gb.bus.ppu.frames(), 1);
    assert_ne!(gb.bus.peek(0xff0f) & 1, 0);
    gb.run_cycles(456 * 9);
    assert_eq!(gb.bus.peek(0xff44), 153);
    gb.run_cycles(4);
    assert_eq!(gb.bus.peek(0xff44), 0);
    assert_eq!(gb.bus.ppu.mode(), 1);
}

fn tile(bus: &mut Bus, number: u16, color: u8) {
    for row in 0..8 {
        let base = 0x8000 + number * 16 + row * 2;
        bus.cpu_write(base, if color & 1 == 0 { 0 } else { 0xff });
        bus.cpu_write(base + 1, if color & 2 == 0 { 0 } else { 0xff });
    }
}

fn sprite(bus: &mut Bus, index: u16, data: [u8; 4]) {
    for (i, value) in data.into_iter().enumerate() {
        bus.cpu_write(0xfe00 + index * 4 + i as u16, value);
    }
}

#[test]
fn background_window_and_sprite_priority_limit() {
    let mut gb = program(&[0x76]);
    let bus = &mut gb.bus;
    bus.cpu_write(0xff40, 0);
    bus.cpu_write(0xff47, 0xe4);
    bus.cpu_write(0xff48, 0xe4);
    tile(bus, 0, 1);
    tile(bus, 1, 2);
    tile(bus, 2, 3);
    bus.cpu_write(0xff4a, 1);
    bus.cpu_write(0xff4b, 47);
    bus.cpu_write(0x9c00, 1);
    bus.cpu_write(0x9c01, 1);
    sprite(bus, 0, [17, 15, 2, 0]);
    sprite(bus, 1, [17, 14, 1, 0x80]); // lower X wins, then BG hides it
    sprite(bus, 2, [17, 35, 2, 0]);
    sprite(bus, 3, [17, 35, 1, 0]); // equal X, lower OAM index wins
    for i in 4..10 {
        sprite(bus, i, [17, 0, 2, 0]);
    }
    sprite(bus, 10, [17, 60, 2, 0]); // excluded even though earlier sprites are offscreen
    bus.cpu_write(0xff40, 0xf3);
    gb.run_cycles(452 + 456);
    let frame = gb.bus.ppu.framebuffer();
    assert_eq!(frame.len(), WIDTH * HEIGHT);
    let line = &frame[WIDTH..WIDTH * 2];
    assert_eq!((line[6], line[7], line[14]), (1, 1, 3));
    assert_eq!((line[27], line[40], line[52]), (3, 2, 2));
}

#[test]
fn sprite_8x16_flip_and_second_palette() {
    let mut gb = program(&[0x76]);
    let bus = &mut gb.bus;
    bus.cpu_write(0xff40, 0);
    bus.cpu_write(0xff47, 0xe4);
    bus.cpu_write(0xff49, 0x0c);
    bus.cpu_write(0x803e, 1); // tile 3, row 7, column 7
    sprite(bus, 0, [17, 8, 3, 0x70]); // tile 2/3, flips, OBP1
    bus.cpu_write(0xff40, 0x97);
    gb.run_cycles(452 + 456);
    assert_eq!(gb.bus.ppu.framebuffer()[WIDTH], 3);
    assert_eq!(gb.bus.ppu.framebuffer()[WIDTH + 7], 0);
}

#[test]
fn apu_four_channels_and_pcm_output() {
    let mut gb = program(&[0x76]);
    let bus = &mut gb.bus;
    bus.cpu_write(0xff26, 0);
    assert_eq!(bus.peek(0xff26), 0x70);
    bus.cpu_write(0xff26, 0x80);
    for (address, value) in [
        (0xff12, 0xf3),
        (0xff17, 0xf3),
        (0xff1a, 0x80),
        (0xff1c, 0x20),
        (0xff21, 0xf3),
        (0xff24, 0x77),
        (0xff25, 0xff),
        (0xff14, 0x87),
        (0xff19, 0x87),
        (0xff1e, 0x87),
        (0xff23, 0x80),
    ] {
        bus.cpu_write(address, value);
    }
    assert_eq!(bus.peek(0xff26), 0xff);
    gb.run_cycles(8192);
    assert_eq!(gb.bus.apu.samples().len(), 128);
    assert!(gb.bus.apu.samples().iter().any(|&sample| sample != 0));
    gb.bus.cpu_write(0xff1a, 0);
    assert_eq!(gb.bus.peek(0xff26), 0xfb);
}
