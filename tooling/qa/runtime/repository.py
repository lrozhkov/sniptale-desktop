from __future__ import annotations

import hashlib
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[3]
SHARED_CONTROL_FILES = {
    ".config/nextest.toml",
    ".editorconfig",
    ".gitattributes",
    ".gitignore",
    "AGENTS.md",
    "Cargo.lock",
    "Cargo.toml",
    "deny.toml",
    "rust-toolchain.toml",
    "rustfmt.toml",
}


def _git(*args: str, check: bool = True) -> bytes:
    completed = subprocess.run(["git", *args], cwd=ROOT, check=False, capture_output=True)
    if check and completed.returncode != 0:
        raise RuntimeError(completed.stderr.decode("utf-8", errors="replace").strip())
    return completed.stdout


def revision() -> str:
    value = _git("rev-parse", "--verify", "HEAD", check=False).decode().strip()
    return value or "no-head"


def current_targets() -> list[str]:
    tracked = _git("diff", "--name-only", "--no-renames", "-z", "HEAD", check=False).split(b"\0")
    untracked = _git("ls-files", "--others", "--exclude-standard", "-z").split(b"\0")
    return sorted({item.decode("utf-8", errors="surrogateescape") for item in [*tracked, *untracked] if item})


def is_harness_target(path: str) -> bool:
    return (
        path.startswith("tooling/")
        or path.startswith("docs/tooling/")
        or path.startswith(".agents/")
        or path.startswith(".github/workflows/")
        or path in SHARED_CONTROL_FILES
    )


def harness_targets(targets: list[str] | None = None) -> list[str]:
    return [path for path in (targets or current_targets()) if is_harness_target(path)]


def fingerprint(paths: list[str] | None = None) -> str:
    selected = sorted(set(paths if paths is not None else current_targets()))
    digest = hashlib.sha256()
    digest.update(revision().encode())
    for relative in selected:
        digest.update(relative.encode("utf-8", errors="surrogateescape"))
        digest.update(b"\0")
        path = ROOT / relative
        if path.is_file():
            digest.update(hashlib.sha256(path.read_bytes()).digest())
        else:
            digest.update(b"<deleted>")
    return digest.hexdigest()
