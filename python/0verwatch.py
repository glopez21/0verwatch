#!/usr/bin/env python3
"""0verwatch - Python wrapper for the Rust 0verwatch binary.

Usage:
    0verwatch                       # full text report
    0verwatch --json                # JSON output
    0verwatch -m kernel,memory      # select modules
    0verwatch --score               # only the score

When the Rust binary is on PATH, this forwards all args. Otherwise it
attempts to locate it next to this script or in the cargo target dir.
"""
from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
from pathlib import Path


def find_binary() -> str | None:
    on_path = shutil.which("0verwatch")
    if on_path:
        return on_path
    here = Path(__file__).resolve().parent
    candidates = [
        here.parent / "target" / "release" / "0verwatch",
        here.parent / "target" / "debug" / "0verwatch",
        here / "0verwatch",
    ]
    for c in candidates:
        if c.exists() and os.access(c, os.X_OK):
            return str(c)
    return None


def run(args: list[str]) -> int:
    binary = find_binary()
    if binary is None:
        print("0verwatch: binary not found. Run `cargo build --release` first.", file=sys.stderr)
        return 127
    try:
        proc = subprocess.run([binary, *args])
        return proc.returncode
    except FileNotFoundError:
        print("0verwatch: binary disappeared.", file=sys.stderr)
        return 127


def json_report(args: list[str]) -> dict | None:
    binary = find_binary()
    if binary is None:
        return None
    out = subprocess.run([binary, "--json-pretty", *args], capture_output=True, text=True)
    if out.returncode != 0:
        return None
    try:
        return json.loads(out.stdout)
    except json.JSONDecodeError:
        return None


def main() -> int:
    return run(sys.argv[1:])


if __name__ == "__main__":
    raise SystemExit(main())