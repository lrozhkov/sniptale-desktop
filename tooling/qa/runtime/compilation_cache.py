"""Invalidate application outputs after restoring hosted Linux dependency caches."""

from __future__ import annotations

import subprocess

from .repository import ROOT


CLEAN_COMMANDS = [
    ["cargo", "clean", "--locked", "--package", "sniptale-native-agent", "--target-dir", "target"],
    [
        "cargo",
        "clean",
        "--locked",
        "--package",
        "sniptale-native-agent",
        "--target",
        "x86_64-pc-windows-msvc",
        "--target-dir",
        "target",
    ],
    ["cargo", "clean", "--locked", "--package", "sniptale-native-agent", "--target-dir", "target/llvm-cov-target"],
]


def main() -> int:
    for command in CLEAN_COMMANDS:
        subprocess.run(command, cwd=ROOT, check=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
