"""Bounded source-image cleanup and one-time golden disk compaction."""

from __future__ import annotations

import os
import shutil
import time
from pathlib import Path

from config import LANES, LabConfig
from errors import IntegrityError, LabError
from paths import LabPaths
from state import read_json, write_json
from system import run, sha256_file

STORAGE_FORMAT = "qcow2-zlib-v1"


def _qemu_img(paths: LabPaths) -> str:
    candidate = shutil.which("qemu-img")
    if candidate:
        return candidate
    bundled = paths.cache / "host-tools/bin/qemu-img"
    if bundled.is_file() and not bundled.is_symlink():
        return str(bundled)
    raise LabError("qemu-img is unavailable; run tooling/desktop/e2e-lab bootstrap")


def allocated_bytes(path: Path) -> int:
    metadata = path.stat(follow_symlinks=False)
    return metadata.st_blocks * 512


def compact_unsealed_disk(paths: LabPaths, disk: Path) -> tuple[int, int]:
    """Replace a private provisioning disk with a checked compressed flat QCOW2."""
    if disk.is_symlink() or not disk.is_file():
        raise IntegrityError("golden disk compaction input is not a regular owner-local file")
    before = allocated_bytes(disk)
    temporary = disk.with_name(f".{disk.name}.compacted.partial")
    temporary.unlink(missing_ok=True)
    try:
        run([
            _qemu_img(paths), "convert", "-q", "-c", "-O", "qcow2",
            "-o", "compression_type=zlib", str(disk), str(temporary),
        ], timeout=3600)
        run([_qemu_img(paths), "check", "-q", str(temporary)], timeout=1800)
        os.chmod(temporary, 0o600)
        os.replace(temporary, disk)
    finally:
        temporary.unlink(missing_ok=True)
    return before, allocated_bytes(disk)


def _recover_interrupted_swap(directory: Path) -> None:
    disk = directory / "disk.qcow2"
    identity = directory / "identity.json"
    backup_disk = directory / ".disk.qcow2.precompact"
    backup_identity = directory / ".identity.json.precompact"
    if not backup_disk.exists() and not backup_identity.exists():
        return
    if not backup_disk.is_file() or not backup_identity.is_file():
        raise IntegrityError(f"incomplete golden compaction recovery state for {directory.name}")
    current = read_json(identity) if identity.is_file() else None
    if (
        current is not None
        and disk.is_file()
        and current.get("storage_format") == STORAGE_FORMAT
        and current.get("disk_sha256") == sha256_file(disk)
    ):
        backup_disk.unlink()
        backup_identity.unlink()
        return
    disk.unlink(missing_ok=True)
    identity.unlink(missing_ok=True)
    os.replace(backup_disk, disk)
    os.replace(backup_identity, identity)


def compact_existing_goldens(
    paths: LabPaths,
    _config: LabConfig,
    manager: object,
) -> dict[str, int]:
    """Compact legacy goldens once, under the controller lock and with rollback files."""
    for lane in LANES:
        current = getattr(manager, "current")(lane, clean_stale=False)
        orphans = getattr(manager, "orphan_pids")(lane)
        if current is not None or orphans:
            return {"goldens_compacted": 0, "golden_bytes_reclaimed": 0, "goldens_skipped_running": 1}

    compacted = 0
    reclaimed = 0
    for lane in LANES:
        directory = paths.scoped(paths.golden / lane)
        if not directory.exists():
            continue
        if directory.is_symlink() or not directory.is_dir():
            raise IntegrityError(f"golden directory is redirected for {lane}")
        _recover_interrupted_swap(directory)
        disk = directory / "disk.qcow2"
        identity_path = directory / "identity.json"
        identity = read_json(identity_path)
        if identity is None or not disk.is_file() or disk.is_symlink() or identity_path.is_symlink():
            raise IntegrityError(f"golden identity is incomplete for {lane}")
        if identity.get("storage_format") == STORAGE_FORMAT:
            continue
        if identity.get("disk_sha256") != sha256_file(disk):
            raise IntegrityError(f"golden identity mismatch before compaction for {lane}")

        before = allocated_bytes(disk)
        compacted_disk = directory / ".disk.qcow2.compacted.partial"
        compacted_identity = directory / ".identity.json.compacted.partial"
        backup_disk = directory / ".disk.qcow2.precompact"
        backup_identity = directory / ".identity.json.precompact"
        for stale in (compacted_disk, compacted_identity, backup_disk, backup_identity):
            stale.unlink(missing_ok=True)
        try:
            run([
                _qemu_img(paths), "convert", "-q", "-c", "-O", "qcow2",
                "-o", "compression_type=zlib", str(disk), str(compacted_disk),
            ], timeout=3600)
            run([_qemu_img(paths), "check", "-q", str(compacted_disk)], timeout=1800)
            identity["disk_sha256"] = sha256_file(compacted_disk)
            identity["storage_format"] = STORAGE_FORMAT
            write_json(compacted_identity, identity)
            os.chmod(compacted_disk, 0o444)
            os.chmod(compacted_identity, 0o444)
            os.replace(disk, backup_disk)
            os.replace(identity_path, backup_identity)
            try:
                os.replace(compacted_disk, disk)
                os.replace(compacted_identity, identity_path)
                if read_json(identity_path).get("disk_sha256") != sha256_file(disk):
                    raise IntegrityError(f"compacted golden verification failed for {lane}")
            except BaseException:
                disk.unlink(missing_ok=True)
                identity_path.unlink(missing_ok=True)
                os.replace(backup_disk, disk)
                os.replace(backup_identity, identity_path)
                raise
            backup_disk.unlink()
            backup_identity.unlink()
        finally:
            compacted_disk.unlink(missing_ok=True)
            compacted_identity.unlink(missing_ok=True)
        after = allocated_bytes(disk)
        compacted += 1
        reclaimed += max(0, before - after)
    return {"goldens_compacted": compacted, "golden_bytes_reclaimed": reclaimed, "goldens_skipped_running": 0}


def cleanup_source_images(paths: LabPaths) -> dict[str, int]:
    """Remove reproducible install media once referenced by a sealed golden."""
    referenced: set[str] = set()
    for lane in LANES:
        identity = read_json(paths.golden / lane / "identity.json")
        if identity and isinstance(identity.get("source_sha256"), str):
            referenced.add(identity["source_sha256"])
    removed = 0
    reclaimed = 0
    now = time.time()
    for image in paths.images.iterdir() if paths.images.exists() else ():
        if image.is_symlink() or not image.is_file() or image.name.endswith(".partial"):
            continue
        referenced_image = any(digest in image.name for digest in referenced)
        stale_unreferenced = now - image.stat().st_mtime > 86400
        if referenced_image or stale_unreferenced:
            size = allocated_bytes(image)
            paths.scoped(image).unlink()
            removed += 1
            reclaimed += size
    return {"source_images_removed": removed, "source_image_bytes_reclaimed": reclaimed}
