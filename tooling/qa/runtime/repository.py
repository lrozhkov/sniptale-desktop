from __future__ import annotations

import hashlib
import os
import re
import stat
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
    base = os.environ.get("SNIPTALE_QA_BASE", "")
    if base:
        if not re.fullmatch(r"[0-9a-f]{40}", base):
            raise RuntimeError("QA diff base must be a full commit SHA")
        ancestor = _git("merge-base", base, "HEAD").decode().strip()
        tracked += _git("diff", "--name-only", "--no-renames", "-z", ancestor, "HEAD").split(b"\0")
    untracked = _git("ls-files", "--others", "--exclude-standard", "-z").split(b"\0")
    return sorted({item.decode("utf-8", errors="surrogateescape") for item in [*tracked, *untracked] if item})


def is_harness_target(path: str) -> bool:
    if path.endswith(".md"):
        return path == "AGENTS.md" or path.startswith(".agents/")
    return (
        path.startswith("tooling/")
        or path.startswith("docs/tooling/")
        or path.startswith(".agents/")
        or path.startswith(".github/workflows/")
        or path in SHARED_CONTROL_FILES
    )


def harness_targets(targets: list[str] | None = None) -> list[str]:
    return [path for path in (current_targets() if targets is None else targets) if is_harness_target(path)]


def check_plan(targets: list[str] | None = None, *, full: bool = False) -> list[str]:
    """Select fixed proof families. Unknown paths and policy changes fail closed to full proof."""
    paths = current_targets() if targets is None else targets
    all_checks = {"docs", "scripts", "ui", "rust", "long-tests", "full"}
    if full or not paths:
        return sorted(all_checks)
    result = {"docs"}
    policy = {
        "tooling/qa/runtime/repository.py",
        "tooling/qa/runtime/state.py",
        "tooling/qa/runtime/cli.py",
        "tooling/qa/runtime/checkpoint_checks.py",
        "tooling/qa/runtime/fast_checks.py",
        "tooling/qa/runtime/lib.sh",
        *[
            f"tooling/qa/wrappers/{name}.sh"
            for name in ("checkpoint", "closeout", "release", "fast", "alpha-proof", "release-harness")
        ],
    }
    dependencies = {
        "Cargo.toml",
        "Cargo.lock",
        "deny.toml",
        "rust-toolchain.toml",
        "ui/package.json",
        "ui/package-lock.json",
        "tooling/configs/qa/tool-versions.json",
        "tooling/configs/qa/toolchain-lock.json",
    }
    for path in paths:
        if path in dependencies:
            result.update(all_checks | {"dependencies"})
        elif path in policy or path.startswith((".github/", "tooling/configs/", "tooling/qa/installers/")):
            result.update(all_checks)
        elif path == "AGENTS.md" or path.startswith(".agents/"):
            result.add("scripts")
        elif path.endswith(".md"):
            continue
        elif path.startswith("ui/"):
            result.add("ui")
        elif path.startswith("resources/locales/"):
            result.update({"ui", "rust"})
        elif path.startswith("src/") and path.endswith(".rs"):
            result.add("rust")
            if path.startswith(
                ("src/protocol/", "src/ipc/", "src/transfer/", "src/telemetry/", "src/assets/", "src/app/bridge/")
            ):
                result.add("long-tests")
        elif path.startswith(("tooling/", ".agents/")) and path.endswith((".py", ".sh", ".ps1", ".psm1", ".psd1")):
            result.add("scripts")
        else:
            result.update(all_checks)
    return sorted(result)


def fingerprint(paths: list[str] | None = None) -> str:
    selected = sorted(set(paths if paths is not None else current_targets()))
    digest = hashlib.sha256()
    digest.update(revision().encode())
    for relative in selected:
        digest.update(relative.encode("utf-8", errors="surrogateescape"))
        digest.update(b"\0")
        path = ROOT / relative
        if path.is_symlink():
            digest.update(b"<symlink>" + os.readlink(path).encode())
        elif path.is_file():
            digest.update(str(stat.S_IMODE(path.stat().st_mode)).encode())
            digest.update(hashlib.sha256(path.read_bytes()).digest())
        else:
            digest.update(b"<deleted>")
    return digest.hexdigest()
