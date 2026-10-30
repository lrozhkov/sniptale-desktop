"""Bounded, diagnostic-only Windows work images; acceptance always starts from golden."""

from __future__ import annotations

import json
import os
import re
import time
from pathlib import Path

from errors import IntegrityError, LabError, ResourceError
from limits import directory_allocated_bytes
from progress import emit
from state import read_json, write_json
from system import run, sha256_file

MAX_AGE = 14 * 86400
MAX_BYTES = 12 * 1024**3
CONVERSION_TIMEOUT = 600


def cache_root(paths):
    location = paths.cache / "windows-work-image"
    if location.is_symlink():
        raise IntegrityError("work image cache directory is redirected")
    return paths.scoped(location)


def restore(paths, qemu_img, golden, golden_sha256, overlay, disk_gib):
    root = cache_root(paths)
    disk = root / "disk.qcow2"
    try:
        stamp = read_json(root / "image.json")
        required = {
            "schema_version",
            "mode",
            "release_admissible",
            "golden_sha256",
            "image_sha256",
            "created_epoch",
            "expires_epoch",
            "source_session_id",
        }
        if (
            not isinstance(stamp, dict)
            or set(stamp) != required
            or type(stamp["schema_version"]) is not int
            or stamp["schema_version"] != 1
        ):
            raise IntegrityError("work image cache metadata missing or invalid")
        if (
            stamp["mode"] != "development-session"
            or stamp["release_admissible"] is not False
            or stamp["golden_sha256"] != golden_sha256
        ):
            raise IntegrityError("work image cache provenance changed")
        if (
            any(type(stamp[key]) is not int for key in ("created_epoch", "expires_epoch"))
            or not stamp["created_epoch"] <= time.time() < stamp["expires_epoch"]
            or not 0 < stamp["expires_epoch"] - stamp["created_epoch"] <= MAX_AGE
        ):
            raise IntegrityError("work image cache expired")
        if (
            not isinstance(stamp["source_session_id"], str)
            or re.fullmatch(r"[0-9]{8}T[0-9]{6}Z(?:-[0-9]{3})?", stamp["source_session_id"]) is None
            or not isinstance(stamp["image_sha256"], str)
            or re.fullmatch(r"[0-9a-f]{64}", stamp["image_sha256"]) is None
        ):
            raise IntegrityError("invalid work image cache identity")
        if (
            disk.is_symlink()
            or not disk.is_file()
            or disk.stat().st_size > MAX_BYTES
            or sha256_file(disk) != stamp["image_sha256"]
        ):
            raise IntegrityError("work image cache checksum mismatch")
        info = json.loads(run([qemu_img, "info", "--output=json", str(disk)], timeout=60).stdout)
        details = info.get("format-specific", {}).get("data", {})
        if (
            info.get("format") != "qcow2"
            or info.get("backing-filename") != str(golden)
            or info.get("virtual-size") != disk_gib * 1024**3
            or info.get("encrypted")
            or any("file" in key for key in details)
        ):
            raise IntegrityError("work image cache backing or format invalid")
    except (IntegrityError, LabError, OSError, ValueError, KeyError, TypeError):
        emit("work-image-cache", status="miss")
        return None
    # Convert only allocated blocks and retain the original immutable backing. No cache chain.
    run(
        [qemu_img, "convert", "-q", "-O", "qcow2", "-B", str(golden), "-F", "qcow2", str(disk), str(overlay)],
        timeout=CONVERSION_TIMEOUT,
    )
    emit("work-image-cache", status="restored")
    return stamp


def store(paths, qemu_img, golden, overlay, golden_sha256, session_id, maximum_cache_gib):
    root = cache_root(paths)
    root.mkdir(parents=True, exist_ok=True)
    candidate = root / "pending.qcow2"
    disk = root / "disk.qcow2"
    if candidate.is_symlink() or disk.is_symlink():
        raise IntegrityError("work image cache file is redirected")
    candidate.unlink(missing_ok=True)
    try:
        run(
            [
                qemu_img,
                "convert",
                "-q",
                "-c",
                "-O",
                "qcow2",
                "-o",
                "compression_type=zstd",
                "-B",
                str(golden),
                "-F",
                "qcow2",
                str(overlay),
                str(candidate),
            ],
            timeout=CONVERSION_TIMEOUT,
        )
        run([qemu_img, "check", "-q", str(candidate)], timeout=180)
        existing = disk.stat().st_blocks * 512 if disk.exists() else 0
        if (
            candidate.stat().st_size > MAX_BYTES
            or directory_allocated_bytes(paths.cache) - existing > maximum_cache_gib * 1024**3
        ):
            raise ResourceError("work image exceeds the persistent compilation cache budget")
        created = int(time.time())
        stamp = dict(
            schema_version=1,
            mode="development-session",
            release_admissible=False,
            golden_sha256=golden_sha256,
            image_sha256=sha256_file(candidate),
            created_epoch=created,
            expires_epoch=created + MAX_AGE,
            source_session_id=session_id,
        )
        os.replace(candidate, disk)
        write_json(root / "image.json", stamp)
        return stamp
    finally:
        candidate.unlink(missing_ok=True)
