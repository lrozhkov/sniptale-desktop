"""Bounded run evidence and cache accounting."""

from __future__ import annotations

from pathlib import Path

from config import Limits
from errors import ResourceError


def directory_bytes(path: Path) -> int:
    return sum(item.stat().st_size for item in path.rglob("*") if item.is_file()) if path.exists() else 0


def directory_allocated_bytes(path: Path) -> int:
    return (
        sum(
            item.stat(follow_symlinks=False).st_blocks * 512
            for item in path.rglob("*")
            if item.is_file() and not item.is_symlink()
        )
        if path.exists()
        else 0
    )


def enforce_run_limits(run_dir: Path, limits: Limits) -> dict[str, int]:
    categories = {
        "logs": (limits.max_log_mib, directory_bytes(run_dir / "logs")),
        "screenshots": (limits.max_screenshot_mib, directory_bytes(run_dir / "screenshots")),
        "recordings": (limits.max_recording_mib, directory_bytes(run_dir / "recordings")),
    }
    for name, (maximum_mib, actual) in categories.items():
        if actual > maximum_mib * 1024 * 1024:
            raise ResourceError(f"{name} evidence exceeds {maximum_mib} MiB run limit")
    return {name: actual for name, (_, actual) in categories.items()}
