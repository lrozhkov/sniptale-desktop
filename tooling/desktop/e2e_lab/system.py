"""Bounded subprocesses, host resources, checksums, and downloads."""

from __future__ import annotations

import hashlib
import os
import shutil
import signal
import selectors
import subprocess
import sys
import tempfile
import time
import urllib.parse
import urllib.request
from contextlib import contextmanager
from pathlib import Path
from typing import Sequence

from errors import IntegrityError, LabError, ResourceError, TimeoutExpired
from receipt import redact_text


MAX_STREAM_LINE_BYTES = 64 * 1024


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def require_sha256(path: Path, expected: str) -> str:
    if len(expected) != 64 or any(char not in "0123456789abcdefABCDEF" for char in expected):
        raise IntegrityError("expected SHA-256 must be 64 hexadecimal characters")
    actual = sha256_file(path)
    if actual.lower() != expected.lower():
        raise IntegrityError(f"SHA-256 mismatch for {path.name}: expected {expected.lower()}, got {actual}")
    return actual


def download_https(
    url: str,
    destination: Path,
    expected: str,
    *,
    timeout: int = 3600,
    max_bytes: int = 16 * 1024 * 1024 * 1024,
) -> Path:
    parsed = urllib.parse.urlparse(url)
    if parsed.scheme != "https" or not parsed.hostname:
        raise IntegrityError("downloads require an absolute HTTPS URL")
    destination.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    partial = destination.with_suffix(destination.suffix + ".partial")
    request = urllib.request.Request(url, headers={"User-Agent": "SniptaleDesktopE2ELab/1"})
    try:
        started = time.monotonic()
        with _wall_clock_deadline(timeout), urllib.request.urlopen(request, timeout=min(timeout, 120)) as response, partial.open("wb") as output:
            final = urllib.parse.urlparse(response.geturl())
            if final.scheme != "https":
                raise IntegrityError("download redirected away from HTTPS")
            if final.hostname != parsed.hostname:
                raise IntegrityError("download redirected to an unexpected host")
            content_length = response.headers.get("Content-Length")
            if content_length is not None:
                try:
                    declared_length = int(content_length)
                except ValueError as exc:
                    raise IntegrityError("download returned an invalid Content-Length") from exc
                if declared_length > max_bytes:
                    raise ResourceError(f"download exceeds {max_bytes} byte limit")
            downloaded = 0
            while chunk := response.read(1024 * 1024):
                if time.monotonic() - started >= timeout:
                    raise TimeoutExpired(f"download timed out after {timeout}s")
                downloaded += len(chunk)
                if downloaded > max_bytes:
                    raise ResourceError(f"download exceeds {max_bytes} byte limit")
                output.write(chunk)
    except Exception:
        partial.unlink(missing_ok=True)
        raise
    require_sha256(partial, expected)
    os.replace(partial, destination)
    return destination


@contextmanager
def _wall_clock_deadline(seconds: int):
    def expired(_signum: int, _frame: object) -> None:
        raise TimeoutExpired(f"download timed out after {seconds}s")

    previous_handler = signal.getsignal(signal.SIGALRM)
    signal.signal(signal.SIGALRM, expired)
    previous_timer = signal.setitimer(signal.ITIMER_REAL, seconds)
    try:
        yield
    finally:
        signal.setitimer(signal.ITIMER_REAL, *previous_timer)
        signal.signal(signal.SIGALRM, previous_handler)


def run(
    command: Sequence[str],
    *,
    timeout: int,
    cwd: Path | None = None,
    env: dict[str, str] | None = None,
    log: Path | None = None,
    stream: bool = False,
    max_output_bytes: int = 64 * 1024 * 1024,
) -> subprocess.CompletedProcess[str]:
    merged = os.environ.copy()
    if env:
        merged.update(env)
    with tempfile.TemporaryFile() as output:
        process = subprocess.Popen(
            list(command), cwd=cwd, env=merged, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
            start_new_session=True,
        )
        assert process.stdout is not None
        selector = selectors.DefaultSelector()
        selector.register(process.stdout, selectors.EVENT_READ)
        deadline = time.monotonic() + timeout
        captured = 0
        pending = b""
        discarding_overlong = False
        try:
            while True:
                for key, _mask in selector.select(timeout=0.05):
                    chunk = os.read(key.fd, 8192)
                    if not chunk:
                        selector.unregister(process.stdout)
                        continue
                    captured += len(chunk)
                    if captured > max_output_bytes:
                        os.killpg(process.pid, signal.SIGKILL)
                        process.wait()
                        raise ResourceError(
                            f"command output exceeded {max_output_bytes} byte limit: {command[0]}"
                        )
                    output.write(chunk)
                    if stream:
                        pending += chunk
                        while b"\n" in pending:
                            line, pending = pending.split(b"\n", 1)
                            if discarding_overlong or len(line) > MAX_STREAM_LINE_BYTES:
                                if not discarding_overlong:
                                    print(
                                        "[desktop-e2e] overlong guest output line redacted",
                                        file=sys.stderr,
                                        flush=True,
                                    )
                                discarding_overlong = False
                                continue
                            print(
                                redact_text(line.decode("utf-8", "replace")),
                                file=sys.stderr,
                                flush=True,
                            )
                        if discarding_overlong:
                            pending = b""
                        elif len(pending) > MAX_STREAM_LINE_BYTES:
                            print(
                                "[desktop-e2e] overlong guest output line redacted",
                                file=sys.stderr,
                                flush=True,
                            )
                            pending = b""
                            discarding_overlong = True
                if process.poll() is not None and not selector.get_map():
                    break
                if time.monotonic() >= deadline:
                    os.killpg(process.pid, signal.SIGKILL)
                    process.wait()
                    raise TimeoutExpired(f"command timed out after {timeout}s: {command[0]}")
        finally:
            selector.close()
            process.stdout.close()
        if stream and pending and not discarding_overlong:
            print(
                redact_text(pending.decode("utf-8", "replace")),
                file=sys.stderr,
                flush=True,
            )
        output.seek(0)
        stdout = output.read(max_output_bytes + 1).decode("utf-8", "replace")
        if len(stdout.encode("utf-8")) > max_output_bytes:
            raise ResourceError(f"command output exceeded {max_output_bytes} byte limit: {command[0]}")
    completed = subprocess.CompletedProcess(list(command), process.returncode, stdout, None)
    if log is not None:
        log.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
        log.write_text(redact_text(completed.stdout), encoding="utf-8")
    if completed.returncode != 0:
        tail = redact_text("\n".join(completed.stdout.splitlines()[-20:]), max_length=4096)
        raise LabError(f"command failed ({completed.returncode}): {command[0]}\n{tail}")
    return completed


def terminate_process(pid: int, *, timeout: int) -> bool:
    try:
        os.kill(pid, signal.SIGTERM)
    except ProcessLookupError:
        return False
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            os.kill(pid, 0)
        except ProcessLookupError:
            return True
        time.sleep(0.2)
    try:
        os.kill(pid, signal.SIGKILL)
    except ProcessLookupError:
        return True
    return True


def available_memory_mib() -> int:
    for line in Path("/proc/meminfo").read_text(encoding="utf-8").splitlines():
        if line.startswith("MemAvailable:"):
            return int(line.split()[1]) // 1024
    raise ResourceError("cannot determine available host memory")


def validate_resources(path: Path, *, min_free_gib: int, min_memory_mib: int) -> dict[str, int]:
    usage = shutil.disk_usage(path)
    free_gib = usage.free // (1024 ** 3)
    memory_mib = available_memory_mib()
    if free_gib < min_free_gib:
        raise ResourceError(f"desktop E2E lab needs {min_free_gib} GiB free; only {free_gib} GiB available")
    if memory_mib < min_memory_mib:
        raise ResourceError(f"desktop E2E lab needs {min_memory_mib} MiB available RAM; only {memory_mib} MiB available")
    return {"free_disk_gib": free_gib, "available_memory_mib": memory_mib}


def wait_for_resources(
    path: Path,
    *,
    min_free_gib: int,
    min_memory_mib: int,
    timeout: int,
) -> dict[str, int]:
    deadline = time.monotonic() + timeout
    while True:
        try:
            return validate_resources(
                path, min_free_gib=min_free_gib, min_memory_mib=min_memory_mib,
            )
        except ResourceError:
            if time.monotonic() >= deadline:
                raise
            time.sleep(min(2, max(0, deadline - time.monotonic())))
