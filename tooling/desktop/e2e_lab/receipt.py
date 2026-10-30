"""Machine-readable run evidence with bounded redacted details."""

from __future__ import annotations

import json
import re
import time
from pathlib import Path
from typing import Any

from errors import IntegrityError

REDACT_PATTERNS = (
    re.compile(r"(?i)data:(?:image|audio|video)/[^\s,;]+;base64,[a-z0-9+/=_-]+"),
    re.compile(r"(?i)(?:typed\s+text|clipboard)\s*:\s*[^\r\n]+"),
    re.compile(r"(?i)(?:authorization|cookie)\s*:\s*[^\r\n]+"),
    re.compile(r"(?i)bearer\s+[a-z0-9._~+/=-]+"),
    re.compile(r"(?i)\"?(?:token|password|secret|authorization|cookie)\"?\s*[:=]\s*\"?[^\s,;}\"]+\"?"),
    re.compile(r"(?i)https?://[^\s\"'}]+"),
    re.compile(r"(?:[A-Za-z]:\\|/home/|/mnt/[a-z]/)[^\s\"']+"),
    re.compile(r"chrome-extension://[a-p]{32}/?"),
    re.compile(r"(?i)extensionId\s*[:=]\s*[a-p]{32}"),
    re.compile(r"(?i)\"?(?:recordingId|operationId)\"?\s*:\s*\"?[^\s,;}\"]+\"?"),
    re.compile(
        r"(?i)(?:recording-(?!(?:system-audio|microphone)(?=$|[\s\"',;}]))|"
        r"screenshot-|capture-|operation-)[a-z0-9._:-]+"
    ),
)
SENSITIVE_KEY = re.compile(r"(?i)(?:token|password|secret|authorization|cookie|api[_-]?key)")


def redact_text(value: str, *, max_length: int | None = None) -> str:
    result = value
    for pattern in REDACT_PATTERNS:
        result = pattern.sub("[redacted]", result)
    return result if max_length is None else result[:max_length]


def redact(value: str) -> str:
    return redact_text(value, max_length=4096)


def redact_data(value: Any, *, depth: int = 0) -> Any:
    if depth > 8:
        return "[redacted-depth]"
    if isinstance(value, str):
        return redact(value)
    if value is None or isinstance(value, (bool, int, float)):
        return value
    if isinstance(value, list):
        return [redact_data(item, depth=depth + 1) for item in value[:256]]
    if isinstance(value, dict):
        result = {}
        for key, item in list(value.items())[:256]:
            safe_key = redact(str(key))[:128]
            result[safe_key] = "[redacted]" if SENSITIVE_KEY.search(str(key)) else redact_data(item, depth=depth + 1)
        return result
    return "[redacted-type]"


def create_run_dir(runs: Path) -> tuple[str, Path]:
    base = time.strftime("%Y%m%dT%H%M%SZ", time.gmtime())
    for suffix in range(1000):
        run_id = base if suffix == 0 else f"{base}-{suffix:03d}"
        path = runs / run_id
        try:
            path.mkdir(parents=True, mode=0o700)
        except FileExistsError:
            continue
        for name in ("logs", "screenshots", "recordings"):
            (path / name).mkdir(mode=0o700)
        return run_id, path
    raise IntegrityError("could not allocate a unique run id")


def validate_receipt(value: dict[str, Any]) -> None:
    required = {
        "schema_version",
        "run_id",
        "source",
        "commit_sha",
        "dirty_state_sha256",
        "artifact_manifest_sha256",
        "evidence_manifest_sha256",
        "artifacts",
        "lanes",
        "snapshots",
        "test_matrix",
        "durations_ms",
        "teardown",
        "result",
    }
    missing = required - set(value)
    if missing:
        raise IntegrityError(f"receipt missing fields: {', '.join(sorted(missing))}")
    if value["schema_version"] != 1 or value["result"] not in {"pass", "fail"}:
        raise IntegrityError("receipt schema version or result is invalid")
    if (
        not isinstance(value["lanes"], list)
        or not value["lanes"]
        or not all(isinstance(lane, str) and lane for lane in value["lanes"])
        or len(set(value["lanes"])) != len(value["lanes"])
    ):
        raise IntegrityError("receipt must identify at least one lane")
    if not isinstance(value["artifacts"], list) or not isinstance(value["snapshots"], dict):
        raise IntegrityError("receipt artifacts or snapshot identities are invalid")
    run_mode = value.get("run_mode", "full-acceptance")
    if run_mode not in {"full-acceptance", "focused-rerun", "focused-test", "alpha-release-acceptance"}:
        raise IntegrityError("receipt run mode is invalid")
    requested = value.get("requested_cases", [])
    if (
        not isinstance(requested, list)
        or len(requested) > 256
        or not all(isinstance(item, str) and item for item in requested)
        or len(requested) != len(set(requested))
    ):
        raise IntegrityError("receipt requested case selection is invalid")
    if run_mode == "focused-rerun" and (not requested or not isinstance(value.get("retry_of"), str)):
        raise IntegrityError("focused rerun receipt must bind requested cases and its source run")


def write_receipt(path: Path, value: dict[str, Any]) -> None:
    validate_receipt(value)
    path.write_text(json.dumps(value, sort_keys=True, indent=2) + "\n", encoding="utf-8")
