"""Atomic state, exclusive control locking, and stale-process validation."""

from __future__ import annotations

import fcntl
import json
import os
import stat
import tempfile
import time
from contextlib import AbstractContextManager, suppress
from pathlib import Path
from typing import Any
from collections.abc import Mapping

from errors import IntegrityError, ResourceError


def process_matches(pid: int, marker: str, required_path: str | None = None) -> bool:
    if pid <= 1:
        return False
    try:
        cmdline = Path(f"/proc/{pid}/cmdline").read_bytes().replace(b"\0", b" ").decode("utf-8", "replace")
    except OSError:
        return False
    return marker in cmdline and "qemu-system" in cmdline and (required_path is None or required_path in cmdline)


def read_json(path: Path) -> dict[str, Any] | None:
    try:
        descriptor = os.open(path, os.O_RDONLY | os.O_CLOEXEC | os.O_NOFOLLOW)
    except FileNotFoundError:
        return None
    except OSError as exc:
        raise IntegrityError(f"corrupted state file: {path.name}") from exc
    try:
        with os.fdopen(descriptor, "r", encoding="utf-8") as stream:
            metadata = os.fstat(stream.fileno())
            if not stat.S_ISREG(metadata.st_mode) or metadata.st_size > 1024 * 1024:
                raise IntegrityError(f"state file type or size is invalid: {path.name}")
            value = json.loads(stream.read(1024 * 1024 + 1))
    except (OSError, json.JSONDecodeError) as exc:
        raise IntegrityError(f"corrupted state file: {path.name}") from exc
    if not isinstance(value, dict):
        raise IntegrityError(f"state file is not an object: {path.name}")
    return value


def write_json(path: Path, value: Mapping[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    fd, temporary = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as stream:
            json.dump(value, stream, sort_keys=True, indent=2)
            stream.write("\n")
            stream.flush()
            os.fsync(stream.fileno())
        os.chmod(temporary, 0o600)
        os.replace(temporary, path)
    finally:
        with suppress(FileNotFoundError):
            os.unlink(temporary)


class LabLock(AbstractContextManager["LabLock"]):
    def __init__(self, path: Path, operation: str, *, nonblocking: bool = True) -> None:
        self.path = path
        self.operation = operation
        self.nonblocking = nonblocking
        self._stream = None

    def __enter__(self) -> LabLock:
        self.path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
        try:
            descriptor = os.open(
                self.path,
                os.O_RDWR | os.O_CREAT | os.O_CLOEXEC | os.O_NOFOLLOW,
                0o600,
            )
        except OSError as exc:
            raise IntegrityError("controller lock is redirected or unreadable") from exc
        metadata = os.fstat(descriptor)
        if (
            not stat.S_ISREG(metadata.st_mode)
            or metadata.st_uid != os.getuid()
            or metadata.st_mode & (stat.S_IRWXG | stat.S_IRWXO)
        ):
            os.close(descriptor)
            raise IntegrityError("controller lock ownership or mode is invalid")
        self._stream = os.fdopen(descriptor, "r+", encoding="utf-8")
        flags = fcntl.LOCK_EX | (fcntl.LOCK_NB if self.nonblocking else 0)
        try:
            fcntl.flock(self._stream.fileno(), flags)
        except BlockingIOError as exc:
            self._stream.seek(0)
            holder = self._stream.read().strip() or "unknown holder"
            self._stream.close()
            self._stream = None
            raise ResourceError(f"desktop E2E lab is busy: {holder}") from exc
        self._stream.seek(0)
        self._stream.truncate()
        json.dump({"pid": os.getpid(), "operation": self.operation, "started_epoch": int(time.time())}, self._stream)
        self._stream.flush()
        os.fsync(self._stream.fileno())
        return self

    def __exit__(self, exc_type, exc_value, traceback) -> None:
        if self._stream is not None:
            self._stream.seek(0)
            self._stream.truncate()
            self._stream.flush()
            fcntl.flock(self._stream.fileno(), fcntl.LOCK_UN)
            self._stream.close()
            self._stream = None
