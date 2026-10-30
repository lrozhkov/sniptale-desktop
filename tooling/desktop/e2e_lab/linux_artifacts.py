"""Typed transport contract for Linux desktop E2E artifacts."""

from __future__ import annotations

import re
import subprocess
from dataclasses import dataclass
from pathlib import Path, PurePosixPath

from errors import IntegrityError, LabError
from probe_package import build as build_probe_package

PACKAGE_NAME = re.compile(r"^[a-z0-9][a-z0-9+.-]{0,127}$")


@dataclass(frozen=True)
class LinuxArtifactPlan:
    kind: str
    remote_name: str
    package_name: str
    executable: str


def build_current_linux_artifact(
    root: Path,
    *,
    profile: str,
    source_archive: Path,
    output: Path,
) -> list[Path]:
    if profile not in {"dev", "release"} or not source_archive.is_file():
        raise IntegrityError("Linux build adapter received an invalid profile or source bundle")
    artifact = output / "sniptale-e2e-probe_1.0.0_all.deb"
    build_probe_package(root, artifact)
    return [artifact]


def inspect_linux_artifact(path: Path) -> LinuxArtifactPlan:
    suffix = path.suffix.lower()
    if suffix == ".appimage":
        return LinuxArtifactPlan("appimage-v1", "artifact.AppImage", "portable", "./artifact.AppImage")
    if suffix != ".deb":
        raise LabError("Linux E2E requires a .deb or .AppImage artifact")
    package = _dpkg(path, "--field", "Package").strip()
    if not PACKAGE_NAME.fullmatch(package):
        raise IntegrityError("Linux package has an invalid Debian package name")
    contents = _dpkg(path, "--contents")
    executables: list[str] = []
    for line in contents.splitlines():
        fields = line.split(maxsplit=5)
        if len(fields) != 6 or not fields[0].startswith("-") or "x" not in fields[0]:
            continue
        relative = PurePosixPath(fields[5].removeprefix("./"))
        if len(relative.parts) == 3 and relative.parts[:2] == ("usr", "bin"):
            executables.append(f"/{relative.as_posix()}")
    if len(executables) != 1:
        raise IntegrityError("Linux .deb E2E contract requires exactly one executable in /usr/bin")
    return LinuxArtifactPlan("deb-v1", "artifact.deb", package, executables[0])


def _dpkg(path: Path, *arguments: str) -> str:
    if not arguments:
        raise ValueError("dpkg-deb operation is required")
    completed = subprocess.run(
        ["dpkg-deb", arguments[0], str(path), *arguments[1:]],
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=120,
        check=False,
    )
    if completed.returncode != 0:
        raise IntegrityError(f"invalid Debian package metadata: {completed.stderr[-512:]}")
    return completed.stdout
