use dmg_emu::{
    ppu::{HEIGHT, WIDTH},
    GameBoy,
};
use std::{env, fs, path::PathBuf, process::ExitCode};

const USAGE: &str = "Uso: dmg-emu <rom.gb> [opções]
  --cycles N                Limite em T-states (padrão 300000000)
  --frames N                Para após N VBlanks
  --protocol blargg|mooneye  Interpreta o resultado da ROM
  --serial-contains TEXTO   Para quando o texto aparece na serial
  --frame ARQUIVO.pgm       Salva o framebuffer final em PGM P5
  --expect-frame ARQUIVO    Compara com um PGM produzido pela CLI
  --battery ARQUIVO.sav     Carrega/grava RAM de cartucho com bateria
  --help                   Exibe esta ajuda";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Protocol {
    Blargg,
    Mooneye,
}

struct Options {
    rom: PathBuf,
    cycles: u64,
    frames: Option<u64>,
    protocol: Option<Protocol>,
    needle: Option<Vec<u8>>,
    frame: Option<PathBuf>,
    expect_frame: Option<PathBuf>,
    battery: Option<PathBuf>,
}

#[derive(Clone, Copy)]
struct Outcome {
    passed: bool,
    reason: &'static str,
}

fn parse() -> Result<Option<Options>, String> {
    let mut args = env::args_os().skip(1);
    let Some(rom) = args.next() else {
        return Ok(None);
    };
    if rom == "--help" || rom == "-h" {
        return Ok(None);
    }
    let mut options = Options {
        rom: rom.into(),
        cycles: 300_000_000,
        frames: None,
        protocol: None,
        needle: None,
        frame: None,
        expect_frame: None,
        battery: None,
    };
    while let Some(option) = args.next() {
        if option == "--help" || option == "-h" {
            return Ok(None);
        }
        let name = option.to_string_lossy();
        let value = args
            .next()
            .ok_or_else(|| format!("valor ausente em {name}"))?;
        let number = || {
            value
                .to_string_lossy()
                .parse::<u64>()
                .ok()
                .filter(|&n| n > 0)
                .ok_or_else(|| format!("use um inteiro positivo em {name}"))
        };
        match name.as_ref() {
            "--cycles" => options.cycles = number()?,
            "--frames" => options.frames = Some(number()?),
            "--protocol" => {
                options.protocol = Some(match value.to_str() {
                    Some("blargg") => Protocol::Blargg,
                    Some("mooneye") => Protocol::Mooneye,
                    _ => return Err("protocolo deve ser blargg ou mooneye".into()),
                })
            }
            "--serial-contains" => {
                options.needle = Some(
                    value
                        .into_string()
                        .ok()
                        .filter(|text| !text.is_empty())
                        .ok_or("texto serial vazio ou inválido")?
                        .into_bytes(),
                )
            }
            "--frame" => options.frame = Some(value.into()),
            "--expect-frame" => options.expect_frame = Some(value.into()),
            "--battery" => options.battery = Some(value.into()),
            _ => return Err(format!("opção desconhecida {name}")),
        }
    }
    if options.protocol.is_some() && options.needle.is_some() {
        return Err("use somente uma condição serial de parada".into());
    }
    Ok(Some(options))
}

fn contains(buffer: &[u8], needle: &[u8]) -> bool {
    buffer.windows(needle.len()).any(|window| window == needle)
}

fn blargg_status(gb: &GameBoy) -> Option<u8> {
    if [0xde, 0xb0, 0x61]
        .into_iter()
        .enumerate()
        .all(|(i, byte)| gb.bus.peek(0xa001 + i as u16) == byte)
    {
        return Some(gb.bus.peek(0xa000));
    }
    None
}

fn mooneye_breakpoint(gb: &GameBoy) -> Option<Outcome> {
    let pc = gb.cpu.pc;
    // The last LD B,B is immediately followed by JR to itself.
    if gb.bus.peek(pc.wrapping_sub(1)) != 0x40
        || gb.bus.peek(pc) != 0x18
        || gb.bus.peek(pc.wrapping_add(1)) != 0xfe
    {
        return None;
    }
    let cpu = &gb.cpu;
    let regs = [cpu.b, cpu.c, cpu.d, cpu.e, cpu.h, cpu.l];
    match regs {
        [3, 5, 8, 13, 21, 34] => Some(Outcome {
            passed: true,
            reason: "Mooneye, registradores Fibonacci no LD B,B",
        }),
        [0x42, 0x42, 0x42, 0x42, 0x42, 0x42] => Some(Outcome {
            passed: false,
            reason: "Mooneye, registradores 42 no LD B,B",
        }),
        _ => None,
    }
}

fn serial_status(protocol: Protocol, buffer: &[u8]) -> Option<Outcome> {
    match protocol {
        Protocol::Blargg => {
            if buffer.ends_with(b"Passed\n") || buffer.ends_with(b"Passed all tests\n") {
                return Some(Outcome {
                    passed: true,
                    reason: "Blargg, mensagem serial",
                });
            }
            let previous_line = buffer
                .strip_suffix(b"\n")
                .and_then(|lines| lines.rsplit(|&byte| byte == b'\n').next());
            if previous_line
                .is_some_and(|line| line.starts_with(b"Failed") || line.ends_with(b"failed."))
            {
                return Some(Outcome {
                    passed: false,
                    reason: "Blargg, mensagem serial",
                });
            }
        }
        Protocol::Mooneye => {
            if buffer.ends_with(&[3, 5, 8, 13, 21, 34]) {
                return Some(Outcome {
                    passed: true,
                    reason: "Mooneye, assinatura Fibonacci",
                });
            }
            if buffer.ends_with(&[0x42; 6]) {
                return Some(Outcome {
                    passed: false,
                    reason: "Mooneye, assinatura de falha",
                });
            }
        }
    }
    None
}

fn pgm(gb: &GameBoy) -> Vec<u8> {
    let mut bytes = format!("P5\n{WIDTH} {HEIGHT}\n255\n").into_bytes();
    bytes.extend(
        gb.bus
            .ppu
            .framebuffer()
            .iter()
            .map(|shade| 255 - 85 * shade),
    );
    bytes
}

fn run(options: Options) -> Result<bool, Box<dyn std::error::Error>> {
    let mut gb = GameBoy::new(fs::read(options.rom)?)?;
    if let Some(path) = &options.battery {
        if gb.bus.cartridge.battery_ram().is_none() {
            return Err("cartucho sem RAM de bateria".into());
        }
        match fs::read(path) {
            Ok(bytes) if !gb.bus.cartridge.restore_battery_ram(&bytes) => {
                return Err("tamanho do save incompatível com o cartucho".into());
            }
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error.into()),
            _ => {}
        }
    }
    let expects_signal =
        options.frames.is_some() || options.protocol.is_some() || options.needle.is_some();
    let mut outcome = Outcome {
        passed: !expects_signal,
        reason: "limite de ciclos",
    };
    let mut serial_len = 0;
    let mut blargg_running = false;
    while gb.bus.cycles() < options.cycles {
        gb.step();
        if gb.cpu.locked {
            outcome = Outcome {
                passed: false,
                reason: "CPU travada por opcode inválido",
            };
            break;
        }
        if options.protocol == Some(Protocol::Blargg) {
            if let Some(status) = blargg_status(&gb) {
                if status == 0x80 {
                    blargg_running = true;
                } else if blargg_running {
                    outcome = Outcome {
                        passed: status == 0,
                        reason: "Blargg, status do cartucho",
                    };
                    break;
                }
            }
        }
        if options.protocol == Some(Protocol::Mooneye) {
            if let Some(result) = mooneye_breakpoint(&gb) {
                outcome = result;
                break;
            }
        }
        let serial = gb.bus.serial();
        if serial_len != serial.len() {
            serial_len = serial.len();
            if let Some(result) = options
                .protocol
                .and_then(|protocol| serial_status(protocol, serial))
            {
                outcome = result;
                break;
            }
            if options
                .needle
                .as_ref()
                .is_some_and(|needle| contains(serial, needle))
            {
                outcome = Outcome {
                    passed: true,
                    reason: "texto serial encontrado",
                };
                break;
            }
        }
        if options
            .frames
            .is_some_and(|frames| gb.bus.ppu.frames() >= frames)
        {
            outcome = Outcome {
                passed: options.protocol.is_none() && options.needle.is_none(),
                reason: "limite de quadros",
            };
            break;
        }
    }
    if options.frame.is_some() || options.expect_frame.is_some() {
        let image = pgm(&gb);
        if let Some(expected) = options.expect_frame {
            if fs::read(expected)? != image {
                outcome = Outcome {
                    passed: false,
                    reason: "framebuffer difere do PGM esperado",
                };
            } else if outcome.passed {
                outcome.reason = "framebuffer igual ao PGM esperado";
            }
        }
        if let Some(path) = options.frame {
            fs::write(path, image)?;
        }
    }
    if let Some(path) = options.battery {
        if let Some(ram) = gb.bus.cartridge.battery_ram() {
            fs::write(path, ram)?;
        }
    }
    println!(
        "Resultado: {}",
        if outcome.passed { "PASS" } else { "FAIL" }
    );
    println!("T-states: {}", gb.bus.cycles());
    println!("Quadros: {}", gb.bus.ppu.frames());
    println!("Diagnóstico: {}", outcome.reason);
    println!(
        "CPU: PC={:04X} SP={:04X} AF={:04X} BC={:04X} DE={:04X} HL={:04X}",
        gb.cpu.pc,
        gb.cpu.sp,
        gb.cpu.af(),
        gb.cpu.bc(),
        gb.cpu.de(),
        gb.cpu.hl()
    );
    if blargg_running {
        let message: Vec<_> = (0xa004..=0xbfff)
            .map(|address| gb.bus.peek(address))
            .take_while(|&byte| byte != 0)
            .collect();
        println!(
            "Cartucho: status={:02X} texto={:?}",
            gb.bus.peek(0xa000),
            String::from_utf8_lossy(&message)
        );
    }
    println!(
        "Serial (texto): {:?}",
        String::from_utf8_lossy(gb.bus.serial())
    );
    print!("Serial (hex):");
    for byte in gb.bus.serial() {
        print!(" {byte:02x}");
    }
    println!();
    Ok(outcome.passed)
}

fn main() -> ExitCode {
    match parse() {
        Ok(None) => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        Ok(Some(options)) => match run(options) {
            Ok(true) => ExitCode::SUCCESS,
            Ok(false) => ExitCode::from(1),
            Err(error) => {
                eprintln!("Erro: {error}");
                ExitCode::from(2)
            }
        },
        Err(error) => {
            eprintln!("Erro: {error}\n{USAGE}");
            ExitCode::from(2)
        }
    }
}
