from __future__ import annotations

import hashlib
import json
import os
import re
import stat
import tempfile
from datetime import datetime, timedelta, timezone
from pathlib import Path


MAX_LOG_BYTES = 256 * 1024
MAX_RUNS = 500
MAX_INVALID = 25
MAX_ORPHAN_LOGS = 25
MAX_JSON_BYTES = 4 * 1024 * 1024
RETENTION_AGE = timedelta(days=30)
STALE_RUN_AGE = timedelta(hours=6)
TRUNCATION_MARKER = b"\n[qa-observability: log truncated]\n"
SENSITIVE_ASSIGNMENT = re.compile(
    r"(?i)\b("
    r"[A-Z0-9_-]*(?:TOKEN|SECRET|PASSWORD|PASSPHRASE|SIGNING[_-]?KEY|PRIVATE[_-]?KEY|"
    r"CERTIFICATE|AUTHORIZATION|COOKIE|API[_-]?KEY)[A-Z0-9_-]*|"
    r"EXTENSION[_-]?(?:ID|ORIGIN)|CONTROLLER[_-]?LEASE[_-]?ID|CONNECTION[_-]?ID|"
    r"(?:RECORDING|SCREENSHOT|CAPTURE|OPERATION|MEDIA|DESTINATION)[_-]?ID|ASSET[_-]?JOB[_-]?ID"
    r")[\"']?\s*[:=]\s*(\"[^\"\r\n]*\"|'[^'\r\n]*'|[^\s,;]+)"
)
WINDOWS_USER_PATH = re.compile(r"(?i)\b[A-Z]:\\Users\\[^\\\s\"']+(?:\\[^\s\"']*)?")
MEDIA_DATA_URI = re.compile(
    r"(?i)\bdata:(?:image|audio|video|application/octet-stream)/?[^\s,;]*;base64,"
    r"[A-Z0-9+/=_-]+"
)
SENSITIVE_URL = re.compile(r"(?i)\b(?:https?|chrome-extension)://[^\s\"']+")
NATIVE_MESSAGE_PAYLOAD = re.compile(r"(?im)^[^\r\n]*\{[^\r\n]*[\"']type[\"']\s*:\s*[\"']extension\.[^\r\n]*$")
AUTHORIZATION_CREDENTIAL = re.compile(r"(?i)\b(authorization)\s*[:=]\s*(?:bearer|basic)\s+[^\s,;]+")
TYPED_OR_CLIPBOARD = re.compile(
    r"(?im)\b(typed(?:\s+text)?|clipboard)\s*[:=]\s*[^\r\n]*?"
    r"(?=\s+\b(?:typed(?:\s+text)?|clipboard)\s*[:=]|$)"
)
GENERATED_MEDIA_ID = re.compile(
    r"(?i)\b(?:recording|screenshot|capture|operation|media|asset)-"
    r"(?:[0-9a-f]{16,}|secret-[a-z0-9._-]+)\b"
)


def utc_now() -> datetime:
    return datetime.now(timezone.utc)


def iso_now() -> str:
    return utc_now().isoformat().replace("+00:00", "Z")


def _reject_symlink_ancestors(path: Path) -> None:
    current = path
    while True:
        if current.is_symlink():
            raise OSError(f"private QA path is redirected: {current}")
        if current.parent == current:
            return
        current = current.parent


def ensure_private_directory(path: Path) -> None:
    _reject_symlink_ancestors(path)
    path.mkdir(parents=True, exist_ok=True, mode=0o700)
    _reject_symlink_ancestors(path)
    try:
        path.chmod(0o700)
    except OSError:
        pass


def atomic_write_json(path: Path, value: object) -> None:
    ensure_private_directory(path.parent)
    if path.is_symlink():
        raise OSError(f"private QA file is redirected: {path}")
    descriptor, temporary = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    try:
        with os.fdopen(descriptor, "w", encoding="utf-8") as handle:
            json.dump(value, handle, indent=2, sort_keys=True)
            handle.write("\n")
            handle.flush()
            os.fsync(handle.fileno())
        try:
            os.chmod(temporary, 0o600)
        except OSError:
            pass
        os.replace(temporary, path)
        try:
            path.chmod(0o600)
        except OSError:
            pass
    finally:
        try:
            os.unlink(temporary)
        except FileNotFoundError:
            pass


def _read_private_bytes(path: Path, maximum: int) -> bytes:
    _reject_symlink_ancestors(path)
    flags = os.O_RDONLY | getattr(os, "O_CLOEXEC", 0) | getattr(os, "O_NOFOLLOW", 0)
    descriptor = os.open(path, flags)
    try:
        metadata = os.fstat(descriptor)
        if not stat.S_ISREG(metadata.st_mode) or metadata.st_size > maximum:
            raise OSError(f"private QA file type or size is invalid: {path}")
        with os.fdopen(descriptor, "rb", closefd=False) as handle:
            return handle.read(maximum + 1)
    finally:
        os.close(descriptor)


def read_json(path: Path) -> dict[str, object]:
    value = json.loads(_read_private_bytes(path, MAX_JSON_BYTES).decode("utf-8"))
    if not isinstance(value, dict):
        raise ValueError(f"expected JSON object: {path}")
    return value


def sanitize_text(text: str, repository_root: Path, sensitive_values: list[str] | None = None) -> str:
    sanitized = text.replace(str(repository_root), "<repo>")
    home = str(Path.home())
    if home and home != str(repository_root):
        sanitized = sanitized.replace(home, "<home>")
    sanitized = WINDOWS_USER_PATH.sub("<user-path>", sanitized)
    sanitized = NATIVE_MESSAGE_PAYLOAD.sub("<native-messaging-payload>", sanitized)
    sanitized = MEDIA_DATA_URI.sub("<media-payload>", sanitized)
    sanitized = SENSITIVE_URL.sub("<url>", sanitized)
    sanitized = AUTHORIZATION_CREDENTIAL.sub(lambda match: f"{match.group(1)}=<redacted>", sanitized)
    sanitized = TYPED_OR_CLIPBOARD.sub(lambda match: f"{match.group(1)}=<redacted>", sanitized)
    sanitized = GENERATED_MEDIA_ID.sub("<media-id>", sanitized)
    for value in sensitive_values or []:
        if len(value) >= 3:
            sanitized = sanitized.replace(value, "<redacted>")
    return SENSITIVE_ASSIGNMENT.sub(lambda match: f"{match.group(1)}=<redacted>", sanitized)


def append_bounded_log(
    path: Path,
    text: str,
    repository_root: Path,
    sensitive_values: list[str] | None = None,
) -> None:
    ensure_private_directory(path.parent)
    payload = sanitize_text(text, repository_root, sensitive_values).encode("utf-8", errors="replace")
    _reject_symlink_ancestors(path)
    flags = os.O_APPEND | os.O_WRONLY | os.O_CREAT | getattr(os, "O_CLOEXEC", 0) | getattr(os, "O_NOFOLLOW", 0)
    descriptor = os.open(path, flags, 0o600)
    try:
        metadata = os.fstat(descriptor)
        if not stat.S_ISREG(metadata.st_mode):
            raise OSError(f"private QA log is not a regular file: {path}")
        fchmod = getattr(os, "fchmod", None)
        if fchmod is not None:
            try:
                fchmod(descriptor, 0o600)
            except OSError:
                pass
        current_size = metadata.st_size
        content_limit = MAX_LOG_BYTES - len(TRUNCATION_MARKER)
        if current_size > content_limit:
            # Preserve room for a machine-detectable marker even when an earlier writer filled the
            # tail of the file. This also repairs logs created by older writer versions.
            os.ftruncate(descriptor, content_limit)
            payload = TRUNCATION_MARKER
        else:
            remaining_content = content_limit - current_size
            if len(payload) > remaining_content:
                payload = payload[:remaining_content] + TRUNCATION_MARKER
        remaining_payload = memoryview(payload)
        while remaining_payload:
            written = os.write(descriptor, remaining_payload)
            if written <= 0:
                raise OSError(f"could not append private QA log: {path}")
            remaining_payload = remaining_payload[written:]
    finally:
        os.close(descriptor)
    try:
        path.chmod(0o600)
    except OSError:
        pass


def log_metadata(path: Path, relative: str) -> dict[str, object]:
    payload = _read_private_bytes(path, MAX_LOG_BYTES) if path.exists() or path.is_symlink() else b""
    return {
        "path": relative,
        "digest": hashlib.sha256(payload).hexdigest(),
        "byteCount": len(payload),
        "truncated": TRUNCATION_MARKER.strip() in payload,
    }


def _parse_timestamp(value: object) -> datetime | None:
    if not isinstance(value, str):
        return None
    try:
        return datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError:
        return None


def _safe_log_path(root: Path, value: object) -> tuple[Path, str]:
    if not isinstance(value, str):
        raise ValueError("run record log path is missing")
    relative = Path(value)
    if relative.is_absolute() or ".." in relative.parts or relative.parts[:2] != (".tmp", "qa-logs"):
        raise ValueError("run record log path is outside the QA log owner")
    return root / relative, relative.as_posix()


def maintain_storage(root: Path) -> None:
    runs_root = root / ".tmp/qa-observability/runs"
    logs_root = root / ".tmp/qa-logs"
    now = utc_now()
    valid: list[tuple[datetime, Path, dict[str, object]]] = []
    invalid: list[tuple[float, Path]] = []
    referenced_logs: set[str] = set()
    for path in runs_root.glob("**/*.json") if runs_root.exists() else []:
        try:
            record = read_json(path)
            started = _parse_timestamp(record.get("startedAt"))
            if started is None:
                raise ValueError("missing startedAt")
            record_log = record.get("log")
            log_value = record_log.get("path") if isinstance(record_log, dict) else None
            log_path, log_relative = _safe_log_path(root, log_value)
            if record.get("status") == "running" and now - started > STALE_RUN_AGE:
                append_bounded_log(
                    log_path,
                    "\n[qa-observability] stale running record recovered as interrupted\n",
                    root,
                )
                record["status"] = "interrupted"
                record["finishedAt"] = iso_now()
                record["durationMs"] = max(0, int((now - started).total_seconds() * 1000))
                record["exitCode"] = 143
                record["problemIds"] = ["wrapper.stale-run-recovered"]
                record["log"] = log_metadata(log_path, log_relative)
                atomic_write_json(path, record)
            valid.append((started, path, record))
        except (OSError, ValueError, json.JSONDecodeError):
            try:
                invalid.append((path.lstat().st_mtime, path))
            except OSError:
                pass
    valid.sort(key=lambda item: item[0], reverse=True)
    kept: list[tuple[datetime, Path, dict[str, object]]] = []
    for index, (started, path, record) in enumerate(valid):
        if index >= MAX_RUNS or now - started > RETENTION_AGE:
            path.unlink(missing_ok=True)
            record_log = record.get("log")
            log_value = record_log.get("path") if isinstance(record_log, dict) else None
            expired_log, _relative = _safe_log_path(root, log_value)
            expired_log.unlink(missing_ok=True)
            continue
        kept.append((started, path, record))
        log = record.get("log")
        if isinstance(log, dict) and isinstance(log.get("path"), str):
            referenced_logs.add(log["path"])
    invalid.sort(key=lambda item: item[0], reverse=True)
    for index, (mtime, path) in enumerate(invalid):
        if index >= MAX_INVALID or now.timestamp() - mtime > RETENTION_AGE.total_seconds():
            path.unlink(missing_ok=True)
    orphans = (
        sorted(
            (
                (path.lstat().st_mtime, path)
                for path in logs_root.glob("**/*.log")
                if path.relative_to(root).as_posix() not in referenced_logs
            ),
            reverse=True,
        )
        if logs_root.exists()
        else []
    )
    for index, (mtime, path) in enumerate(orphans):
        if index >= MAX_ORPHAN_LOGS or now.timestamp() - mtime > RETENTION_AGE.total_seconds():
            path.unlink(missing_ok=True)
