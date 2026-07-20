from __future__ import annotations

import os
import secrets
import signal
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

from . import lock
from .repository import ROOT, current_targets, fingerprint, revision
from .storage import (
    MAX_LOG_BYTES,
    append_bounded_log,
    atomic_write_json,
    iso_now,
    log_metadata,
    maintain_storage,
    read_json,
    sanitize_text,
)


MAX_STREAM_LINE_BYTES = min(64 * 1024, MAX_LOG_BYTES)


def _run_id() -> str:
    stamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%S.%fZ")
    return f"{stamp}-{secrets.token_hex(6)}"


def _paths(run_id: str) -> tuple[Path, Path, str, str]:
    day = datetime.now(timezone.utc).strftime("%Y-%m-%d")
    run_relative = f".tmp/qa-observability/runs/{day}/{run_id}.json"
    log_relative = f".tmp/qa-logs/{day}/{run_id}.log"
    return ROOT / run_relative, ROOT / log_relative, run_relative, log_relative


def start(wrapper_id: str, mode: str, blocking: bool, sensitive: list[str]) -> dict[str, str]:
    maintain_storage(ROOT)
    run_id = _run_id()
    parent_run_id = os.environ.get("SNIPTALE_QA_RUN_ID") or None
    root_run_id = os.environ.get("SNIPTALE_QA_ROOT_RUN_ID") or parent_run_id or run_id
    run_path, log_path, run_relative, log_relative = _paths(run_id)
    targets = current_targets()
    record: dict[str, object] = {
        "schemaVersion": 1,
        "runId": run_id,
        "rootRunId": root_run_id,
        "parentRunId": parent_run_id,
        "wrapperId": wrapper_id,
        "mode": mode,
        "status": "running",
        "startedAt": iso_now(),
        "finishedAt": None,
        "durationMs": None,
        "exitCode": None,
        "problemIds": [],
        "repository": {
            "revision": revision(),
            "fingerprint": fingerprint(targets),
            "targetFiles": targets,
        },
        "steps": [],
        "log": {"path": log_relative, "digest": "", "byteCount": 0, "truncated": False},
    }
    atomic_write_json(run_path, record)
    append_bounded_log(log_path, f"[{wrapper_id}] started run={run_id}\n", ROOT, sensitive)
    token = os.environ.get("SNIPTALE_QA_LOCK_TOKEN", "")
    if blocking:
        try:
            token, _owner = lock.acquire(ROOT, wrapper_id, run_id, root_run_id)
        except RuntimeError as error:
            append_bounded_log(log_path, f"blocking lock rejected: {error}\n", ROOT, sensitive)
            record["status"] = "problems-found"
            record["finishedAt"] = iso_now()
            record["durationMs"] = 0
            record["exitCode"] = 1
            record["problemIds"] = ["wrapper.blocking-lock-rejected"]
            record["log"] = log_metadata(log_path, log_relative)
            atomic_write_json(run_path, record)
            raise
    return {
        "run_id": run_id,
        "root_run_id": root_run_id,
        "run_path": str(run_path),
        "log_path": str(log_path),
        "run_relative": run_relative,
        "log_relative": log_relative,
        "lock_token": token,
    }


def run_step(
    run_path: Path,
    log_path: Path,
    step_id: str,
    label: str,
    command: list[str],
    sensitive: list[str] | None = None,
    stream_output: bool = False,
) -> int:
    started_at = iso_now()
    started = time.monotonic()
    append_bounded_log(log_path, f"\n[{step_id}] START {label}\n", ROOT, sensitive)
    try:
        process = subprocess.Popen(
            command,
            cwd=ROOT,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
        )
        assert process.stdout is not None
        while True:
            line = process.stdout.readline(MAX_STREAM_LINE_BYTES + 1)
            if not line:
                break
            if len(line) > MAX_STREAM_LINE_BYTES:
                while line and not line.endswith(b"\n"):
                    line = process.stdout.readline(MAX_STREAM_LINE_BYTES + 1)
                decoded = "[qa-observability: overlong output line redacted]\n"
            else:
                decoded = line.decode("utf-8", errors="replace")
            append_bounded_log(
                log_path, decoded, ROOT, sensitive
            )
            if stream_output:
                sys.stderr.write(sanitize_text(decoded, ROOT, sensitive))
                sys.stderr.flush()
        process.stdout.close()
        exit_code = process.wait()
    except OSError as error:
        append_bounded_log(log_path, f"unable to execute command: {error}\n", ROOT, sensitive)
        exit_code = 127
    duration_ms = int((time.monotonic() - started) * 1000)
    outcome = "passed" if exit_code == 0 else "problems-found"
    append_bounded_log(
        log_path, f"[{step_id}] {outcome.upper()} exit={exit_code}\n", ROOT, sensitive
    )
    _record_step(run_path, log_path, step_id, label, started_at, duration_ms, exit_code)
    return exit_code


def begin_shell_step(
    log_path: Path,
    step_id: str,
    label: str,
    sensitive: list[str] | None = None,
) -> tuple[str, int]:
    """Start a step which must execute in the caller's current shell."""
    started_at = iso_now()
    started_ns = time.time_ns()
    append_bounded_log(log_path, f"\n[{step_id}] START {label}\n", ROOT, sensitive)
    return started_at, started_ns


def finish_shell_step(
    run_path: Path,
    log_path: Path,
    step_id: str,
    label: str,
    started_at: str,
    started_ns: int,
    exit_code: int,
    output_path: Path,
    sensitive: list[str] | None = None,
) -> int:
    """Sanitize captured shell-function output and finalize its structured step."""
    try:
        if output_path.exists():
            output = output_path.read_bytes().decode("utf-8", errors="replace")
            append_bounded_log(log_path, output, ROOT, sensitive)
    finally:
        output_path.unlink(missing_ok=True)
    duration_ms = max(0, int((time.time_ns() - started_ns) / 1_000_000))
    outcome = "passed" if exit_code == 0 else "problems-found"
    append_bounded_log(
        log_path, f"[{step_id}] {outcome.upper()} exit={exit_code}\n", ROOT, sensitive
    )
    _record_step(run_path, log_path, step_id, label, started_at, duration_ms, exit_code)
    return exit_code


def _record_step(
    run_path: Path,
    log_path: Path,
    step_id: str,
    label: str,
    started_at: str,
    duration_ms: int,
    exit_code: int,
) -> None:
    outcome = "passed" if exit_code == 0 else "problems-found"
    record = read_json(run_path)
    steps = record.get("steps")
    if not isinstance(steps, list):
        steps = []
    steps.append(
        {
            "stepId": step_id,
            "label": label,
            "outcome": outcome,
            "startedAt": started_at,
            "durationMs": duration_ms,
            "exitCode": exit_code,
            "problemIds": [] if exit_code == 0 else [f"{step_id}.failed"],
        }
    )
    record["steps"] = steps
    record["log"] = log_metadata(log_path, str(record["log"]["path"]))
    atomic_write_json(run_path, record)


def finish(run_path: Path, log_path: Path, exit_code: int) -> dict[str, object]:
    record = read_json(run_path)
    if record.get("status") != "running":
        return record
    append_bounded_log(log_path, f"\n[{record.get('wrapperId')}] finished exit={exit_code}\n", ROOT)
    finished = datetime.now(timezone.utc)
    started_text = str(record.get("startedAt", "")).replace("Z", "+00:00")
    try:
        started = datetime.fromisoformat(started_text)
        duration_ms = int((finished - started).total_seconds() * 1000)
    except ValueError:
        duration_ms = 0
    record["finishedAt"] = finished.isoformat().replace("+00:00", "Z")
    record["durationMs"] = max(0, duration_ms)
    record["exitCode"] = exit_code
    record["status"] = "all-passed" if exit_code == 0 else (
        "interrupted" if exit_code in {128 + signal.SIGINT, 128 + signal.SIGTERM} else "problems-found"
    )
    if exit_code != 0 and not record.get("problemIds"):
        record["problemIds"] = ["wrapper.nonzero-exit"]
    log_relative = str(record.get("log", {}).get("path", ""))
    record["log"] = log_metadata(log_path, log_relative)
    atomic_write_json(run_path, record)
    lock.release(ROOT, str(record.get("runId")))
    maintain_storage(ROOT)
    return record
