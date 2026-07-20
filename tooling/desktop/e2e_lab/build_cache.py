"""Content-addressed desktop build artifacts outside disposable VM overlays."""

from __future__ import annotations

import json
import os
import re
import shutil
import tempfile
from pathlib import Path

from errors import IntegrityError
from paths import LabPaths
from system import sha256_file


CACHE_SCHEMA_VERSION = 1
SHA256 = re.compile(r"^[0-9a-f]{64}$")
MAX_CACHE_FILES = 512
MAX_CACHE_BYTES = 1024 * 1024 * 1024


def _entry(paths: LabPaths, lane: str, profile: str, source_sha256: str) -> Path:
    if not SHA256.fullmatch(source_sha256) or not re.fullmatch(r"[a-z0-9-]+", lane):
        raise IntegrityError("invalid desktop build cache identity")
    if profile not in {"dev", "release"}:
        raise IntegrityError("invalid desktop build cache profile")
    candidate = paths.cache / "builds" / lane / profile / source_sha256
    current = paths.cache
    for part in candidate.relative_to(paths.cache).parts:
        current /= part
        if current.is_symlink():
            raise IntegrityError("desktop build cache path is redirected")
    return paths.scoped(candidate)


def _files(root: Path) -> list[Path]:
    values: list[Path] = []
    total = 0
    for path in sorted(root.rglob("*")):
        if path.name in {"cache.json", "build.log"}:
            continue
        if path.is_symlink() or (path.exists() and not path.is_file() and not path.is_dir()):
            raise IntegrityError("desktop build cache contains an unsafe file type")
        if not path.is_file():
            continue
        values.append(path)
        total += path.stat().st_size
        if len(values) > MAX_CACHE_FILES or total > MAX_CACHE_BYTES:
            raise IntegrityError("desktop build cache exceeds its file or byte limit")
    return values


def restore(
    paths: LabPaths,
    lane: str,
    profile: str,
    source_sha256: str,
    output: Path,
) -> list[Path] | None:
    entry = _entry(paths, lane, profile, source_sha256)
    manifest_path = entry / "cache.json"
    if not manifest_path.is_file() or manifest_path.is_symlink():
        return None
    try:
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        if (
            not isinstance(manifest, dict)
            or manifest.get("schemaVersion") != CACHE_SCHEMA_VERSION
            or manifest.get("sourceSha256") != source_sha256
            or manifest.get("lane") != lane
            or manifest.get("profile") != profile
            or not isinstance(manifest.get("files"), list)
        ):
            raise IntegrityError("desktop build cache manifest identity mismatch")
        actual = _files(entry)
        observed = {
            path.relative_to(entry).as_posix(): {
                "sha256": sha256_file(path),
                "bytes": path.stat().st_size,
            }
            for path in actual
        }
        expected: dict[str, dict[str, object]] = {}
        for item in manifest["files"]:
            if not isinstance(item, dict):
                raise IntegrityError("desktop build cache manifest file entry is invalid")
            relative = item.get("path")
            if (
                not isinstance(relative, str)
                or not relative
                or Path(relative).is_absolute()
                or ".." in Path(relative).parts
                or relative in expected
            ):
                raise IntegrityError("desktop build cache manifest path is invalid")
            expected[relative] = {"sha256": item.get("sha256"), "bytes": item.get("bytes")}
        if expected != observed:
            raise IntegrityError("desktop build cache content does not match its manifest")
    except (OSError, ValueError, json.JSONDecodeError, IntegrityError):
        shutil.rmtree(entry, ignore_errors=True)
        return None

    output.mkdir(parents=True, exist_ok=True, mode=0o700)
    for relative in sorted(observed):
        destination = output / relative
        destination.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
        shutil.copy2(entry / relative, destination)
        try:
            destination.chmod(0o600)
        except OSError:
            pass
    artifacts = sorted(
        path for path in output.rglob("*") if path.is_file() and path.suffix.lower() in {".exe", ".msi"}
    )
    return artifacts or None


def store(
    paths: LabPaths,
    lane: str,
    profile: str,
    source_sha256: str,
    output: Path,
) -> None:
    entry = _entry(paths, lane, profile, source_sha256)
    parent = entry.parent
    parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    temporary: Path | None = Path(tempfile.mkdtemp(prefix=f".{source_sha256}.", dir=parent))
    try:
        files = _files(output)
        manifest_files: list[dict[str, object]] = []
        for source in files:
            relative = source.relative_to(output)
            destination = temporary / relative
            destination.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
            shutil.copy2(source, destination)
            try:
                destination.chmod(0o600)
            except OSError:
                pass
            manifest_files.append(
                {
                    "path": relative.as_posix(),
                    "sha256": sha256_file(destination),
                    "bytes": destination.stat().st_size,
                }
            )
        if not any(str(item["path"]).endswith(".msi") for item in manifest_files):
            raise IntegrityError("desktop build cache requires an MSI artifact")
        manifest = {
            "schemaVersion": CACHE_SCHEMA_VERSION,
            "sourceSha256": source_sha256,
            "lane": lane,
            "profile": profile,
            "files": manifest_files,
        }
        manifest_path = temporary / "cache.json"
        manifest_path.write_text(json.dumps(manifest, sort_keys=True, indent=2) + "\n", encoding="utf-8")
        os.chmod(manifest_path, 0o600)
        if entry.exists():
            shutil.rmtree(entry)
        os.replace(temporary, entry)
        temporary = None
    finally:
        if temporary is not None and temporary.exists():
            shutil.rmtree(temporary, ignore_errors=True)
