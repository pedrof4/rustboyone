#!/usr/bin/env python3
"""Compile the Rust library to WebAssembly and serve the frontend on localhost."""
import argparse
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--port", type=int, default=8000)
    parser.add_argument("--no-build", action="store_true", help="Use the existing web/dmg_emu.wasm.")
    args = parser.parse_args()
    web = ROOT / "web"
    if not args.no_build:
        subprocess.run(
            ["cargo", "build", "--release", "--lib", "--target", "wasm32-unknown-unknown",
             "--target-dir", str(ROOT / "target")],
            cwd=ROOT, check=True,
        )
        shutil.copyfile(ROOT / "target/wasm32-unknown-unknown/release/dmg_emu.wasm", web / "dmg_emu.wasm")
    if not (web / "dmg_emu.wasm").is_file():
        parser.error("WebAssembly ausente. Execute sem --no-build.")
    handler = partial(SimpleHTTPRequestHandler, directory=str(web))
    with ThreadingHTTPServer(("127.0.0.1", args.port), handler) as server:
        print(f"DMG Emu em http://127.0.0.1:{server.server_port}", flush=True)
        try:
            server.serve_forever()
        except KeyboardInterrupt:
            pass


if __name__ == "__main__":
    main()
