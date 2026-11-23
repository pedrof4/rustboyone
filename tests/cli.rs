use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        loop {
            let id = NEXT.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!("dmg-emu-{}-{id}", std::process::id()));
            match fs::create_dir(&path) {
                Ok(()) => return Self(path),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => panic!("failed to create {}: {error}", path.display()),
            }
        }
    }

    fn rom(&self, code: &[u8]) {
        let mut rom = vec![0; 0x8000];
        rom[0x100..0x100 + code.len()].copy_from_slice(code);
        fs::write(self.0.join("test.gb"), rom).unwrap();
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_dmg-emu"))
            .current_dir(&self.0)
            .arg("test.gb")
            .args(args)
            .output()
            .unwrap()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn cli_stops_when_serial_matches_and_times_out_when_missing() {
    let dir = Scratch::new();
    dir.rom(&[0x3e, b'Q', 0xe0, 1, 0x3e, 0x81, 0xe0, 2, 0x76]);
    let output = dir.run(&["--cycles", "10000", "--serial-contains", "Q"]);
    assert!(output.status.success());
    assert!(String::from_utf8(output.stdout)
        .unwrap()
        .contains("Serial (hex): 51"));
    let output = dir.run(&["--cycles", "10000", "--serial-contains", "other"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8(output.stdout)
        .unwrap()
        .contains("T-states: 10000"));
}

#[test]
fn cli_captures_and_compares_pgm_images() {
    let dir = Scratch::new();
    dir.rom(&[0x76]);
    let mut reference = b"P5\n160 144\n255\n".to_vec();
    reference.resize(reference.len() + 160 * 144, 255);
    fs::write(dir.0.join("expected.pgm"), &reference).unwrap();
    let output = dir.run(&[
        "--frames",
        "1",
        "--frame",
        "captured.pgm",
        "--expect-frame",
        "expected.pgm",
    ]);
    assert!(output.status.success());
    assert_eq!(fs::read(dir.0.join("captured.pgm")).unwrap(), reference);
    *reference.last_mut().unwrap() = 0;
    fs::write(dir.0.join("expected.pgm"), reference).unwrap();
    let output = dir.run(&["--frames", "1", "--expect-frame", "expected.pgm"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8(output.stdout)
        .unwrap()
        .contains("framebuffer difere"));
}
