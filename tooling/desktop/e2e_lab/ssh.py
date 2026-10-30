"""Dedicated localhost-only SSH transport for guest control."""

from __future__ import annotations

import base64
import os
import signal
import subprocess
import tempfile
import time
from pathlib import Path
from typing import Callable

from errors import LabError, ResourceError, TimeoutExpired
from paths import LabPaths
from receipt import redact_text
from system import run


def powershell_encoded(script: str) -> list[str]:
    encoded = base64.b64encode(script.encode("utf-16-le")).decode("ascii")
    return [
        "powershell.exe", "-NoLogo", "-NoProfile", "-NonInteractive",
        "-EncodedCommand", encoded,
    ]


class GuestSsh:
    def __init__(self, paths: LabPaths, port: int, user: str) -> None:
        self.paths = paths
        self.port = port
        self.user = user
        self.key = paths.cache / "ssh/lab_ed25519"
        self.known_hosts = paths.cache / "ssh/known_hosts"

    def options(self) -> list[str]:
        return [
            "-i", str(self.key), "-p", str(self.port), "-o", "BatchMode=yes",
            "-o", "StrictHostKeyChecking=accept-new", "-o", f"UserKnownHostsFile={self.known_hosts}",
            "-o", "ConnectTimeout=10", "-o", "ServerAliveInterval=15", "-o", "ServerAliveCountMax=3",
        ]

    def wait(self, timeout: int, *, probe: list[str] | None = None,
             health_check: Callable[[], None] | None = None) -> None:
        probe_command = probe or ["true"]
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if health_check is not None:
                health_check()
            try:
                self.command(probe_command, timeout=15)
                return
            except Exception:
                time.sleep(5)
        raise TimeoutExpired(f"guest SSH did not become ready on loopback port {self.port}")

    def forget_host_key(self) -> None:
        if not self.known_hosts.exists():
            return
        run(
            ["ssh-keygen", "-q", "-f", str(self.known_hosts), "-R", f"[127.0.0.1]:{self.port}"],
            timeout=30,
        )

    def command(
        self,
        command: list[str],
        *,
        timeout: int,
        log: Path | None = None,
        stream: bool = False,
        health_check: Callable[[], None] | None = None,
    ):
        return run(
            ["ssh", *self.options(), f"{self.user}@127.0.0.1", "--", *command],
            timeout=timeout,
            log=log,
            stream=stream,
            **({"health_check": health_check} if health_check is not None else {}),
        )

    def upload(self, source: Path, remote: str, *, timeout: int) -> None:
        scp_options = self.options()
        port_index = scp_options.index("-p")
        scp_options[port_index] = "-P"
        run(["scp", *scp_options, str(source), f"{self.user}@127.0.0.1:{remote}"], timeout=timeout)

    def download(
        self,
        remote: str,
        destination: Path,
        *,
        recursive: bool = False,
        timeout: int,
        max_bytes: int | None = None,
        max_files: int | None = None,
    ) -> None:
        destination.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
        scp_options = self.options()
        port_index = scp_options.index("-p")
        scp_options[port_index] = "-P"
        flags = ["-r"] if recursive else []
        command = ["scp", *flags, *scp_options, f"{self.user}@127.0.0.1:{remote}", str(destination)]
        with tempfile.TemporaryFile() as output:
            process = subprocess.Popen(
                command, stdout=output, stderr=subprocess.STDOUT, start_new_session=True,
            )
            deadline = time.monotonic() + timeout
            while process.poll() is None:
                files, size = _tree_usage(destination)
                if (max_bytes is not None and size > max_bytes) or (max_files is not None and files > max_files):
                    os.killpg(process.pid, signal.SIGKILL)
                    process.wait()
                    raise ResourceError("guest transfer exceeded the local streaming quota")
                if time.monotonic() >= deadline:
                    os.killpg(process.pid, signal.SIGKILL)
                    process.wait()
                    raise TimeoutExpired(f"guest download timed out after {timeout}s")
                time.sleep(0.1)
            if process.returncode != 0:
                output.seek(0)
                detail = output.read(4096).decode("utf-8", "replace")
                raise LabError(f"guest download failed: {redact_text(detail, max_length=4096)}")
        files, size = _tree_usage(destination)
        if (max_bytes is not None and size > max_bytes) or (max_files is not None and files > max_files):
            raise ResourceError("guest transfer exceeded the local streaming quota")


def _tree_usage(path: Path) -> tuple[int, int]:
    if not path.exists():
        return 0, 0
    if path.is_file():
        return 1, path.stat().st_size
    files = 0
    size = 0
    for current, _directories, names in os.walk(path, followlinks=False):
        for name in names:
            candidate = Path(current) / name
            files += 1
            try:
                size += candidate.lstat().st_size
            except FileNotFoundError:
                continue
    return files, size
