#!/usr/bin/env python3
"""Obtain pinned public test ROMs, execute them, and write a Markdown result table."""
import argparse
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass
import hashlib
import os
from pathlib import Path
import subprocess
import tarfile
from tempfile import TemporaryDirectory
from urllib.request import urlopen

ROOT = Path(__file__).resolve().parents[1]
BLARGG = "c240dd7d700e5c0b00a7bbba52b53e4ee67b5f15"
MOONEYE = "mts-20260714-0944-31510e1"
MOONEYE_SHA = "6d4fdda2f1d8d2f5f51b0ff3f6f3cc2fbae047aa395a39c82bda3a0e7cbd2641"
ACID_URL = "https://github.com/mattcurrie/dmg-acid2/releases/download/v1.0/dmg-acid2.gb"
ACID_SHA = "464e14b7d42e7feea0b7ede42be7071dc88913f75b9ffa444299424b63d1dff1"
HALT_FRAME_SHA = "d82792e6823d29ac643fca19fcf17edd156af90974746c1c993e03266d78628b"
ACID_FRAME_SHA = "b2fd578e3564e82e82b4e796d9e4bf735c36fa26a52d55c78132eb225aad72e3"


def digest(data):
    return hashlib.sha256(data).hexdigest()


def fetch(url, target, expected, offline):
    if target.is_file():
        data = target.read_bytes()
    elif offline:
        raise RuntimeError(f"Arquivo ausente em modo offline: {target}")
    else:
        with urlopen(url, timeout=60) as response:
            data = response.read()
    if digest(data) != expected:
        raise RuntimeError(f"SHA-256 inesperado: {target}")
    if not target.exists():
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data)


def prepare_roms(cache, offline):
    cache.mkdir(parents=True, exist_ok=True)
    blargg = cache / "blargg"
    if not blargg.is_dir():
        if offline:
            raise RuntimeError(f"Clone ausente em modo offline: {blargg}")
        with TemporaryDirectory(dir=cache) as temporary:
            clone = Path(temporary) / "blargg"
            subprocess.run(["git", "init", "-q", str(clone)], check=True)
            subprocess.run(["git", "-C", str(clone), "fetch", "-q", "--depth", "1",
                            "https://github.com/retrio/gb-test-roms.git", BLARGG], check=True)
            subprocess.run(["git", "-C", str(clone), "checkout", "-q", "FETCH_HEAD"], check=True)
            clone.rename(blargg)
    revision = subprocess.check_output(["git", "-C", str(blargg), "rev-parse", "HEAD"], text=True).strip()
    if revision != BLARGG:
        raise RuntimeError(f"Revisão de Blargg inesperada: {revision}")
    archive = cache / "mooneye.tar.xz"
    fetch(f"https://gekkio.fi/files/mooneye-test-suite/{MOONEYE}/{MOONEYE}.tar.xz",
          archive, MOONEYE_SHA, offline)
    mooneye = cache / MOONEYE
    if not mooneye.is_dir():
        with TemporaryDirectory(dir=cache) as temporary:
            with tarfile.open(archive, "r:xz") as package:
                package.extractall(temporary, filter="data")
            (Path(temporary) / MOONEYE).rename(mooneye)
    acid = cache / "acid2/dmg-acid2.gb"
    fetch(ACID_URL, acid, ACID_SHA, offline)
    return blargg, mooneye, acid


@dataclass(frozen=True)
class Case:
    name: str
    rom: Path
    protocol: str = ""
    frame_hash: str = ""


def test_cases(blargg, mooneye, acid):
    cases = [Case(f"Blargg/{name}", blargg / name, "blargg") for name in (
        "cpu_instrs/cpu_instrs.gb", "instr_timing/instr_timing.gb",
        "mem_timing/mem_timing.gb", "mem_timing-2/mem_timing.gb",
    )]
    for directory in ("cpu_instrs/individual", "mem_timing/individual", "mem_timing-2/rom_singles"):
        cases.extend(Case(f"Blargg/{rom.relative_to(blargg).as_posix()}", rom, "blargg")
                     for rom in sorted((blargg / directory).glob("*.gb")))
    cases.append(Case("Blargg/halt_bug.gb", blargg / "halt_bug.gb", frame_hash=HALT_FRAME_SHA))
    excluded = {"boot_div-S.gb", "boot_div2-S.gb", "boot_hwio-S.gb", "boot_regs-sgb.gb",
                "boot_regs-sgb2.gb", "boot_regs-mgb.gb", "boot_div-dmg0.gb",
                "boot_hwio-dmg0.gb", "boot_regs-dmg0.gb"}
    for folder in ("acceptance", "emulator-only"):
        cases.extend(Case(f"Mooneye/{rom.relative_to(mooneye).as_posix()}", rom, "mooneye")
                     for rom in sorted((mooneye / folder).rglob("*.gb")) if rom.name not in excluded)
    cases.append(Case("dmg-acid2/dmg-acid2.gb", acid, frame_hash=ACID_FRAME_SHA))
    return cases


def escape(value):
    return value.replace("|", "\\|").replace("\n", " ")


def run_case(binary, case):
    args = [str(binary), str(case.rom), "--cycles", "300000000" if case.protocol == "blargg" else "30000000"]
    if case.protocol:
        args.extend(["--protocol", case.protocol])
    with TemporaryDirectory() as temporary:
        frame = Path(temporary) / "frame.pgm"
        if case.frame_hash:
            args.extend(["--frames", "120", "--frame", str(frame)])
        result = subprocess.run(args, capture_output=True, text=True, encoding="utf-8")
        fields = dict(line.split(": ", 1) for line in result.stdout.splitlines() if ": " in line)
        passed = result.returncode == 0 and fields.get("Resultado") == "PASS"
        diagnostic = fields.get("Diagnóstico") or result.stderr.strip() or "Sem diagnóstico"
        if case.frame_hash:
            actual_hash = digest(frame.read_bytes()) if frame.is_file() else "ausente"
            same = actual_hash == case.frame_hash
            passed = passed and same
            diagnostic = f"PGM SHA-256 {'igual' if same else 'diferente'}: {actual_hash}"
        if "Cartucho" in fields:
            diagnostic += f" · {fields['Cartucho']}"
        if not passed and "CPU" in fields:
            diagnostic += f" · {fields['CPU']}"
        serial = fields.get("Serial (texto)", '""') if case.protocol == "blargg" else fields.get("Serial (hex)", "")
        status = "PASS" if passed else "FAIL"
        line = f"| `{case.name}` | {fields.get('T-states', '—')} | {status} | `{escape(serial or '—')}` | {escape(diagnostic)} |"
        return passed, line


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cache", type=Path, default=ROOT / "work")
    parser.add_argument("--offline", action="store_true", help="Read already acquired ROMs from the cache.")
    parser.add_argument("--jobs", type=int, default=min(4, os.cpu_count() or 1))
    parser.add_argument("--markdown", type=Path, default=ROOT / "work/rom-results.md")
    args = parser.parse_args()
    if args.jobs < 1:
        parser.error("--jobs deve ser maior que zero.")
    cases = test_cases(*prepare_roms(args.cache.resolve(), args.offline))
    subprocess.run(["cargo", "build", "--release", "--target-dir", str(ROOT / "target")], cwd=ROOT, check=True)
    binary = ROOT / "target/release" / ("dmg-emu.exe" if os.name == "nt" else "dmg-emu")
    with ThreadPoolExecutor(max_workers=args.jobs) as pool:
        rows = list(pool.map(lambda case: run_case(binary, case), cases))
    passed = sum(ok for ok, _ in rows)
    args.markdown.parent.mkdir(parents=True, exist_ok=True)
    args.markdown.write_text(
        f"**{passed}/{len(cases)} PASS.**\n\n"
        "| ROM | T-states | Resultado | Serial | Diagnóstico |\n"
        "| --- | ---: | --- | --- | --- |\n"
        + "\n".join(line for _, line in rows) + "\n", encoding="utf-8",
    )
    print(f"{passed}/{len(cases)} PASS — {args.markdown}")
    for ok, line in rows:
        if not ok:
            print(line)
    return 0 if passed == len(cases) else 1


if __name__ == "__main__":
    raise SystemExit(main())
