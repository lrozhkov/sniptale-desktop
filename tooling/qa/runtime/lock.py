from __future__ import annotations

import hashlib
import json
import os
import secrets
from pathlib import Path

from .storage import atomic_write_json, ensure_private_directory, read_json
import contextlib


LOCK_RELATIVE_PATH = Path(".tmp/qa/locks/blocking.json")


def _pid_alive(pid: object) -> bool:
    if not isinstance(pid, int) or pid <= 0:
        return False
    try:
        os.kill(pid, 0)
        return True
    except ProcessLookupError:
        return False
    except PermissionError:
        return True


def _digest(token: str) -> str:
    return hashlib.sha256(token.encode()).hexdigest()


def _claim(path: Path, value: dict[str, object]) -> bool:
    """Publish a complete lock with a no-replace hard link."""
    ensure_private_directory(path.parent)
    candidate = path.parent / f".{path.name}.{os.getpid()}.{secrets.token_hex(8)}.claim"
    atomic_write_json(candidate, value)
    try:
        os.link(candidate, path)
        with contextlib.suppress(OSError):
            path.chmod(0o600)
        return True
    except FileExistsError:
        return False
    finally:
        candidate.unlink(missing_ok=True)


def acquire(root: Path, wrapper_id: str, run_id: str, root_run_id: str) -> tuple[str, bool]:
    path = root / LOCK_RELATIVE_PATH
    inherited = os.environ.get("SNIPTALE_QA_LOCK_TOKEN", "")
    token = secrets.token_hex(32)
    value: dict[str, object] = {
        "schemaVersion": 1,
        "wrapperId": wrapper_id,
        "ownerRunId": run_id,
        "rootRunId": root_run_id,
        "pid": os.getppid(),
        "tokenDigest": _digest(token),
    }
    for _attempt in range(8):
        if path.exists():
            try:
                existing = read_json(path)
            except (OSError, ValueError, json.JSONDecodeError):
                existing = {}
            if not _pid_alive(existing.get("pid")):
                path.unlink(missing_ok=True)
                continue
            if (
                inherited
                and existing.get("tokenDigest") == _digest(inherited)
                and existing.get("rootRunId") == root_run_id
            ):
                return inherited, False
            raise RuntimeError(
                f"blocking QA wrapper already active: {existing.get('wrapperId', 'unknown')} "
                f"pid={existing.get('pid', 'unknown')}"
            )
        if _claim(path, value):
            return token, True
    raise RuntimeError("blocking QA lock changed repeatedly during acquisition")


def release(root: Path, run_id: str) -> None:
    path = root / LOCK_RELATIVE_PATH
    if not path.exists():
        return
    try:
        existing = read_json(path)
    except (OSError, ValueError, json.JSONDecodeError):
        return
    if existing.get("ownerRunId") == run_id:
        path.unlink(missing_ok=True)
