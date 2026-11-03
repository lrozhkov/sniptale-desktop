#!/usr/bin/env python3
from __future__ import annotations

import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[3]
SRC = ROOT / "src"

REQUIRED_OWNER_DIRS = [
    "app",
    "protocol",
    "ipc",
    "settings",
    "assets",
    "tray",
    "hotkeys",
    "capture",
    "recording",
    "audio",
    "telemetry",
    "transfer",
    "install",
    "diagnostics",
]


def fail(messages: list[str]) -> None:
    print("FAIL topology guardrails")
    for message in messages:
        print(f"- {message}")
    raise SystemExit(1)


def module_name(path: str) -> str:
    return Path(path).stem


def role_modules(owner_dir: Path) -> set[str]:
    file_modules = {module_name(path.name) for path in owner_dir.glob("*.rs") if path.name != "mod.rs"}
    directory_modules = {path.name for path in owner_dir.iterdir() if path.is_dir() and (path / "mod.rs").is_file()}
    return file_modules | directory_modules


def role_module_exists(owner_dir: Path, module: str) -> bool:
    return (owner_dir / f"{module}.rs").is_file() or (owner_dir / module / "mod.rs").is_file()


def declared_modules(mod_text: str) -> set[str]:
    return set(re.findall(r"(?m)^\s*(?:pub(?:\(crate\))?\s+)?mod\s+([a-zA-Z_][a-zA-Z0-9_]*)\s*;", mod_text))


def main() -> int:
    errors: list[str] = []
    allowed_entries = set(REQUIRED_OWNER_DIRS) | {"main.rs"}
    for path in SRC.iterdir():
        if path.name not in allowed_entries:
            errors.append(f"unexpected top-level product entry src/{path.name}")

    for owner in REQUIRED_OWNER_DIRS:
        owner_dir = SRC / owner
        if not owner_dir.is_dir():
            errors.append(f"missing owner directory src/{owner}")
            continue
        mod_path = SRC / owner / "mod.rs"
        if not mod_path.is_file():
            errors.append(f"missing owner facade src/{owner}/mod.rs")
            continue
        text = mod_path.read_text(encoding="utf-8")
        declared = declared_modules(text)
        roles = role_modules(owner_dir)
        missing_declarations = sorted(roles - declared)
        stale_declarations = sorted(module for module in declared if not role_module_exists(owner_dir, module))
        if missing_declarations:
            errors.append(f"src/{owner}/mod.rs must declare existing role file(s): " + ", ".join(missing_declarations))
        if stale_declarations:
            errors.append(f"src/{owner}/mod.rs declares missing role file(s): " + ", ".join(stale_declarations))

    if errors:
        fail(errors)
    print("OK topology guardrails")
    return 0


if __name__ == "__main__":
    sys.exit(main())
