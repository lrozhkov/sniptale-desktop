"""Deterministic source bundles and artifact integrity manifests."""

from __future__ import annotations

import hashlib
import gzip
import json
import os
import stat
import subprocess
import tarfile
import shutil
from dataclasses import dataclass
from pathlib import Path, PurePosixPath
from typing import Iterable

from errors import IntegrityError
from system import sha256_file

EXCLUDED_PARTS = {
    ".git",
    ".tmp",
    ".artifacts",
    "target",
    "node_modules",
    ".cache",
    "cache",
    "vm-state",
    "desktop-e2e-lab",
    "__pycache__",
    ".pytest_cache",
    ".idea",
    ".vscode",
}
EXCLUDED_SUFFIXES = {
    ".pem",
    ".key",
    ".pfx",
    ".p12",
    ".jks",
    ".keystore",
    ".iso",
    ".qcow2",
    ".vhd",
    ".vhdx",
    ".vmdk",
    ".mp3",
    ".wav",
    ".flac",
    ".aac",
    ".mp4",
    ".mkv",
    ".mov",
    ".webm",
    ".avi",
}
SENSITIVE_NAMES = {
    ".env",
    ".netrc",
    ".npmrc",
    ".pypirc",
    ".git-credentials",
    "credentials",
    "credentials.json",
    "secrets.json",
    "auth.json",
    "id_rsa",
    "id_ed25519",
}
UNTRACKED_MEDIA_SUFFIXES = {".png", ".jpg", ".jpeg", ".gif", ".webp", ".bmp"}
UNTRACKED_SOURCE_SUFFIXES = {
    ".css",
    ".html",
    ".js",
    ".jsx",
    ".ts",
    ".tsx",
    ".mjs",
    ".cjs",
    ".mts",
    ".cts",
    ".json",
    ".lock",
    ".md",
    ".ps1",
    ".py",
    ".rs",
    ".sh",
    ".svg",
    ".toml",
    ".txt",
    ".wxs",
    ".xml",
    ".yaml",
    ".yml",
}
UNTRACKED_ROOT_TEXT_NAMES = {"LICENSE", "NOTICE"}
MAX_SOURCE_FILES = 20_000
MAX_SOURCE_FILE_BYTES = 64 * 1024 * 1024
MAX_SOURCE_TOTAL_BYTES = 512 * 1024 * 1024


@dataclass(frozen=True)
class SourceFingerprint:
    archive: Path
    sha256: str
    commit_sha: str
    dirty_sha256: str
    file_count: int

    def as_dict(self) -> dict[str, str | int]:
        return {
            "archive_sha256": self.sha256,
            "commit_sha": self.commit_sha,
            "dirty_state_sha256": self.dirty_sha256,
            "file_count": self.file_count,
        }


def excluded(relative: PurePosixPath, *, tracked: bool = False) -> bool:
    parts = set(relative.parts)
    lower_name = relative.name.lower()
    return bool(
        parts & EXCLUDED_PARTS
        or lower_name in SENSITIVE_NAMES
        or relative.suffix.lower() in EXCLUDED_SUFFIXES
        or lower_name.startswith(".env.")
        or (not tracked and ("secret" in lower_name or "credential" in lower_name))
    )


def source_files(root: Path) -> Iterable[tuple[Path, PurePosixPath]]:
    tracked_output = _git(root, "ls-files", "-z")
    tracked = set(tracked_output.split("\0")) if tracked_output != "unavailable" else set()
    visible_output = _git(root, "ls-files", "--cached", "--others", "--exclude-standard", "-z")
    visible = None if visible_output == "unavailable" else set(visible_output.split("\0"))
    visible_directories = {
        PurePosixPath(*PurePosixPath(item).parts[:depth]).as_posix()
        for item in (visible or ())
        for depth in range(1, len(PurePosixPath(item).parts))
    }
    tracked_directories = {
        PurePosixPath(*PurePosixPath(item).parts[:depth]).as_posix()
        for item in tracked
        for depth in range(1, len(PurePosixPath(item).parts))
    }
    for current, directories, files in os.walk(root, topdown=True, followlinks=False):
        current_path = Path(current)
        relative_dir = current_path.relative_to(root)
        traversable = []
        for name in sorted(directories):
            path = current_path / name
            relative = PurePosixPath((relative_dir / name).as_posix())
            if (
                visible is not None
                and relative.as_posix() not in visible
                and relative.as_posix() not in visible_directories
            ):
                continue
            tracked_path = relative.as_posix() in tracked_directories
            if excluded(relative, tracked=tracked_path):
                continue
            if path.is_symlink():
                target = (path.parent / os.readlink(path)).resolve(strict=False)
                if not target.is_relative_to(root.resolve()):
                    raise IntegrityError(f"source symlink escapes repository: {relative}")
                yield path, relative
            else:
                traversable.append(name)
        directories[:] = traversable
        for name in sorted(files):
            path = current_path / name
            relative = PurePosixPath(path.relative_to(root).as_posix())
            if visible is not None and relative.as_posix() not in visible:
                continue
            tracked_path = relative.as_posix() in tracked
            if excluded(relative, tracked=tracked_path):
                continue
            if (
                not tracked_path
                and relative.suffix.lower() not in UNTRACKED_SOURCE_SUFFIXES
                and not (len(relative.parts) == 1 and relative.name in UNTRACKED_ROOT_TEXT_NAMES)
            ):
                executable_script = (
                    not relative.suffix
                    and relative.parts[0] == "tooling"
                    and path.is_file()
                    and path.stat().st_mode & stat.S_IXUSR
                )
                synthetic_resource = relative.suffix.lower() in UNTRACKED_MEDIA_SUFFIXES and relative.parts[0] in {
                    "resources",
                    "contracts",
                }
                if not executable_script and not synthetic_resource:
                    continue
            if (
                relative.suffix.lower() in UNTRACKED_MEDIA_SUFFIXES
                and relative.as_posix() not in tracked
                and relative.parts[0] not in {"resources", "contracts"}
            ):
                continue
            mode = path.lstat().st_mode
            if stat.S_ISREG(mode):
                yield path, relative
            elif stat.S_ISLNK(mode):
                target = (path.parent / os.readlink(path)).resolve(strict=False)
                if not target.is_relative_to(root.resolve()):
                    raise IntegrityError(f"source symlink escapes repository: {relative}")
                yield path, relative
            else:
                raise IntegrityError(f"unsupported special source file: {relative}")


def _git(root: Path, *args: str) -> str:
    result = subprocess.run(
        ["git", *args],
        cwd=root,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
        check=False,
    )
    return result.stdout.strip() if result.returncode == 0 else "unavailable"


def dirty_fingerprint(root: Path) -> str:
    digest = hashlib.sha256()
    digest.update(_git(root, "status", "--porcelain=v1", "-z", "--untracked-files=all").encode())
    digest.update(_git(root, "diff", "--binary", "HEAD", "--").encode())
    tracked_output = _git(root, "ls-files", "-z")
    tracked = set(tracked_output.split("\0")) if tracked_output != "unavailable" else set()
    for path, relative in source_files(root):
        if relative.as_posix() not in tracked:
            digest.update(relative.as_posix().encode())
            if path.is_file():
                digest.update(sha256_file(path).encode())
    return digest.hexdigest()


def source_tree_fingerprint(root: Path) -> dict:
    """Bind external build inputs without generating an unused transport archive."""
    entries = []
    total = 0
    for path, relative in source_files(root):
        info = path.lstat()
        entry = {"name": relative.as_posix(), "mode": stat.S_IMODE(info.st_mode)}
        if stat.S_ISREG(info.st_mode):
            total += info.st_size
            if info.st_size > MAX_SOURCE_FILE_BYTES or total > MAX_SOURCE_TOTAL_BYTES:
                raise IntegrityError("source fingerprint exceeds file or total byte limit")
            entry.update(bytes=info.st_size, sha256=sha256_file(path))
        else:
            entry["link"] = os.readlink(path)
        entries.append(entry)
        if len(entries) > MAX_SOURCE_FILES:
            raise IntegrityError("source fingerprint exceeds file count limit")
    digest = hashlib.sha256(
        json.dumps(sorted(entries, key=lambda entry: entry["name"]), sort_keys=True, separators=(",", ":")).encode()
    ).hexdigest()
    return {
        "source_tree_sha256": digest,
        "commit_sha": _git(root, "rev-parse", "HEAD"),
        "dirty_state_sha256": dirty_fingerprint(root),
        "file_count": len(entries),
    }


def create_source_bundle(root: Path, destination: Path) -> SourceFingerprint:
    destination.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    temporary = destination.with_suffix(destination.suffix + ".partial")
    raw_tar = destination.with_suffix(destination.suffix + ".tar.partial")
    count = 0
    total_bytes = 0
    try:
        with tarfile.open(raw_tar, "w", format=tarfile.PAX_FORMAT) as archive:
            for path, relative in source_files(root):
                info = archive.gettarinfo(str(path), arcname=relative.as_posix())
                if info.isreg():
                    if info.size > MAX_SOURCE_FILE_BYTES:
                        raise IntegrityError(f"source file exceeds 64 MiB limit: {relative}")
                    total_bytes += info.size
                    if total_bytes > MAX_SOURCE_TOTAL_BYTES:
                        raise IntegrityError("source bundle exceeds the 512 MiB uncompressed limit")
                count += 1
                if count > MAX_SOURCE_FILES:
                    raise IntegrityError(f"source bundle exceeds the {MAX_SOURCE_FILES} file limit")
                info.uid = 0
                info.gid = 0
                info.uname = "root"
                info.gname = "root"
                info.mtime = 0
                if info.isreg():
                    with path.open("rb") as stream:
                        archive.addfile(info, stream)
                else:
                    archive.addfile(info)
        with raw_tar.open("rb") as source, temporary.open("wb") as target:
            with gzip.GzipFile(filename="", mode="wb", fileobj=target, mtime=0) as compressed:
                shutil.copyfileobj(source, compressed, length=1024 * 1024)
        raw_tar.unlink()
        os.replace(temporary, destination)
    finally:
        raw_tar.unlink(missing_ok=True)
        temporary.unlink(missing_ok=True)
    return SourceFingerprint(
        archive=destination,
        sha256=sha256_file(destination),
        commit_sha=_git(root, "rev-parse", "HEAD"),
        dirty_sha256=dirty_fingerprint(root),
        file_count=count,
    )


def artifact_manifest(paths: Iterable[Path]) -> dict[str, object]:
    entries = []
    for path in paths:
        resolved = path.resolve(strict=True)
        if not resolved.is_file():
            raise IntegrityError(f"artifact is not a regular file: {path.name}")
        entries.append({"name": resolved.name, "sha256": sha256_file(resolved), "bytes": resolved.stat().st_size})
    return {"schema_version": 1, "artifacts": entries}


def evidence_manifest(run_dir: Path) -> dict[str, object]:
    root = run_dir.resolve(strict=True)
    entries: list[dict[str, object]] = []
    for category in ("logs", "screenshots", "recordings"):
        directory = run_dir / category
        if directory.is_symlink() or not directory.is_dir():
            raise IntegrityError(f"evidence directory is missing or redirected: {category}")
        for path in directory.rglob("*"):
            if path.is_symlink() or not path.resolve(strict=False).is_relative_to(root):
                raise IntegrityError("evidence manifest contains a redirected path")
            if path.is_dir():
                continue
            if not path.is_file():
                raise IntegrityError("evidence manifest contains a special file")
            relative = path.relative_to(run_dir).as_posix()
            entries.append(
                {
                    "relative_path": relative,
                    "category": category,
                    "bytes": path.stat().st_size,
                    "sha256": sha256_file(path),
                }
            )
    entries.sort(key=lambda item: str(item["relative_path"]))
    if len(entries) > 4096:
        raise IntegrityError("evidence manifest exceeds the file-count limit")
    return {"schema_version": 1, "files": entries}


def write_manifest(path: Path, value: dict[str, object]) -> None:
    path.write_text(json.dumps(value, sort_keys=True, indent=2) + "\n", encoding="utf-8")
