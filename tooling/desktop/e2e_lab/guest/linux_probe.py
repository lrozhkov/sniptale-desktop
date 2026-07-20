#!/usr/bin/env python3
"""Deterministic in-guest desktop capability probe."""

from __future__ import annotations

import argparse
import json
import math
import os
import shutil
import signal
import socket
import subprocess
import tempfile
import time
from pathlib import Path


class Probe:
    def __init__(self, lane: str, output: Path, evidence: Path) -> None:
        self.lane = lane
        self.output = output
        self.evidence = evidence
        self.tests: list[dict[str, object]] = []

    def case(self, name: str, function) -> None:
        started = time.monotonic()
        status = "pass"
        detail = ""
        try:
            detail = str(function())
        except Exception as exc:
            status = "fail"
            detail = f"{type(exc).__name__}: {exc}"
        self.tests.append({
            "name": name,
            "status": status,
            "duration_ms": int((time.monotonic() - started) * 1000),
            "detail": detail[:4096],
        })

    @staticmethod
    def command(command: list[str], timeout: int = 60) -> str:
        completed = subprocess.run(
            command, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
            timeout=timeout, check=False,
        )
        if completed.returncode != 0:
            raise RuntimeError(f"{command[0]} exited {completed.returncode}: {completed.stdout[-1000:]}")
        return completed.stdout.strip()

    def session(self) -> str:
        actual = os.environ.get("XDG_SESSION_TYPE", "")
        expected = "wayland" if self.lane == "linux-wayland" else "x11"
        if actual != expected:
            raise RuntimeError(f"expected {expected}, got {actual or 'unset'}")
        return actual

    def network_isolation(self) -> str:
        policy = Path("/run/sniptale-e2e-network-policy")
        if policy.read_text(encoding="utf-8").strip() != "output-drop-v1":
            raise RuntimeError("guest OUTPUT-drop policy was not installed")
        connection = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        connection.settimeout(3)
        try:
            if connection.connect_ex(("1.1.1.1", 443)) == 0:
                raise RuntimeError("guest reached an external TCP endpoint")
        finally:
            connection.close()
        return "guest OUTPUT=DROP and external TCP egress was denied"

    def portal(self) -> str:
        output = self.command(["gdbus", "introspect", "--session", "--dest", "org.freedesktop.portal.Desktop", "--object-path", "/org/freedesktop/portal/desktop"])
        if "org.freedesktop.portal.Screenshot" not in output:
            raise RuntimeError("Screenshot portal interface is absent")
        return "Screenshot portal exported"

    def tray(self) -> str:
        self.command(["gnome-extensions", "enable", "ubuntu-appindicators@ubuntu.com"])
        watcher_deadline = time.monotonic() + 20
        while time.monotonic() < watcher_deadline:
            try:
                self.command([
                    "gdbus", "call", "--session", "--dest", "org.kde.StatusNotifierWatcher",
                    "--object-path", "/StatusNotifierWatcher", "--method", "org.freedesktop.DBus.Properties.Get",
                    "org.kde.StatusNotifierWatcher", "RegisteredStatusNotifierItems",
                ])
                break
            except RuntimeError:
                time.sleep(1)
        else:
            raise RuntimeError("StatusNotifierWatcher did not start after enabling AppIndicator")
        tray_script = Path("/usr/lib/sniptale-e2e-probe/tray.py")
        process = subprocess.Popen(["python3", str(tray_script)], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        try:
            deadline = time.monotonic() + 20
            while time.monotonic() < deadline:
                try:
                    output = self.command([
                        "gdbus", "call", "--session", "--dest", "org.kde.StatusNotifierWatcher",
                        "--object-path", "/StatusNotifierWatcher", "--method", "org.freedesktop.DBus.Properties.Get",
                        "org.kde.StatusNotifierWatcher", "RegisteredStatusNotifierItems",
                    ])
                except RuntimeError:
                    time.sleep(1)
                    continue
                # libappindicator normalizes the application id into its D-Bus
                # object path, replacing hyphens with underscores.
                if "sniptale_e2e_probe" in output:
                    return "StatusNotifierItem registered"
                time.sleep(1)
            raise RuntimeError("probe StatusNotifierItem was not registered")
        finally:
            process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()

    def audio(self) -> str:
        server = self.command(["pactl", "info"])
        if "on PipeWire" not in server:
            raise RuntimeError("Pulse compatibility server is not backed by PipeWire")
        self.command(["systemctl", "--user", "is-active", "wireplumber"])
        original_sink = self.command(["pactl", "get-default-sink"])
        sink_module = self.command(["pactl", "load-module", "module-null-sink", "sink_name=sniptale_test", "sink_properties=device.description=SniptaleSyntheticOutput"])
        source_module = ""
        raw = self.evidence / "synthetic-audio.raw"
        tone = self.evidence / "synthetic-tone.raw"
        try:
            self.command(["pactl", "set-default-sink", "sniptale_test"])
            source_module = self.command(["pactl", "load-module", "module-remap-source", "master=sniptale_test.monitor", "source_name=sniptale_virtual_mic", "source_properties=device.description=SniptaleSyntheticMicrophone"])
            tone.write_bytes(b"".join(
                int(12000 * math.sin(2 * math.pi * 880 * sample / 48000)).to_bytes(2, "little", signed=True)
                for sample in range(48000)
            ))
            recorder = subprocess.Popen([
                "parecord", "--device=sniptale_virtual_mic", "--raw",
                "--rate=48000", "--channels=1", "--format=s16le", str(raw),
            ])
            try:
                time.sleep(1)
                self.command([
                    "pacat", "--playback", "--device=sniptale_test", "--raw",
                    "--rate=48000", "--channels=1", "--format=s16le", str(tone),
                ], timeout=15)
                time.sleep(1)
            finally:
                if recorder.poll() is None:
                    recorder.send_signal(signal.SIGINT)
                try:
                    recorder.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    recorder.kill()
                    recorder.wait(timeout=5)
            if recorder.returncode != 0:
                raise RuntimeError(f"parecord exited {recorder.returncode}")
            if not raw.exists() or raw.stat().st_size < 4096:
                raise RuntimeError("virtual microphone loopback produced no PCM evidence")
            peak = 0
            with raw.open("rb") as stream:
                while chunk := stream.read(1024 * 1024):
                    usable = len(chunk) - (len(chunk) % 2)
                    for offset in range(0, usable, 2):
                        sample = int.from_bytes(chunk[offset:offset + 2], "little", signed=True)
                        peak = max(peak, abs(sample))
            if peak < 100:
                raise RuntimeError("virtual microphone loopback PCM contains no synthetic tone")
            return f"synthetic PCM bytes={raw.stat().st_size}, peak={peak}"
        finally:
            tone.unlink(missing_ok=True)
            cleanup = {"check": False, "stdout": subprocess.DEVNULL, "stderr": subprocess.DEVNULL}
            subprocess.run(["pactl", "set-default-sink", original_sink], **cleanup)
            if source_module.isdigit():
                subprocess.run(["pactl", "unload-module", source_module], **cleanup)
            if sink_module.isdigit():
                subprocess.run(["pactl", "unload-module", sink_module], **cleanup)

    def capture(self) -> str:
        target = self.evidence / "desktop.png"
        if self.lane == "linux-wayland":
            self.command(["gnome-screenshot", "-f", str(target)])
        else:
            self.command(["import", "-window", "root", str(target)])
        if not target.exists() or target.stat().st_size < 1024:
            raise RuntimeError("desktop capture evidence is empty")
        return f"PNG bytes={target.stat().st_size}"

    def browsers(self) -> str:
        chromium = shutil.which("chromium-browser") or shutil.which("chromium") or shutil.which("google-chrome")
        firefox = shutil.which("firefox")
        if not chromium or not firefox:
            raise RuntimeError("Chrome/Chromium and Firefox must both be installed")
        processes: list[subprocess.Popen[bytes]] = []
        profile_root = Path(tempfile.mkdtemp(prefix="sniptale-browser-probe-"))
        try:
            chromium_profile = profile_root / "chromium"
            firefox_profile = profile_root / "firefox"
            chromium_profile.mkdir()
            firefox_profile.mkdir()
            chromium_process = subprocess.Popen(
                [
                    chromium, f"--user-data-dir={chromium_profile}", "--no-first-run",
                    "--no-default-browser-check", "--disable-session-crashed-bubble",
                    f"--ozone-platform={'wayland' if self.lane == 'linux-wayland' else 'x11'}",
                    "--new-window", "data:text/html,<title>Sniptale Chromium Probe</title>",
                ],
                stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True,
            )
            processes.append(chromium_process)
            firefox_env = os.environ.copy()
            firefox_env["MOZ_ENABLE_WAYLAND"] = "1" if self.lane == "linux-wayland" else "0"
            firefox_process = subprocess.Popen(
                [
                    firefox, "--no-remote", "--profile", str(firefox_profile),
                    "--new-window", "data:text/html,<title>Sniptale Firefox Probe</title>",
                ],
                env=firefox_env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                start_new_session=True,
            )
            processes.append(firefox_process)
            deadline = time.monotonic() + 30
            while time.monotonic() < deadline:
                if all(process.poll() is None for process in processes):
                    time.sleep(5)
                    if all(process.poll() is None for process in processes):
                        break
                else:
                    raise RuntimeError("Chrome/Chromium or Firefox exited during headed startup")
            else:
                raise RuntimeError("browser launch readiness timed out")
            target = self.evidence / "browser-launch.png"
            if self.lane == "linux-wayland":
                self.command(["gnome-screenshot", "-f", str(target)])
            else:
                self.command(["import", "-window", "root", str(target)])
            if not target.exists() or target.stat().st_size < 1024:
                raise RuntimeError("browser launch evidence is empty")
            return "Chrome/Chromium and Firefox remained live in the local desktop session"
        finally:
            for process in processes:
                if process.poll() is None:
                    try:
                        os.killpg(process.pid, signal.SIGTERM)
                    except ProcessLookupError:
                        pass
            for process in processes:
                try:
                    process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    try:
                        os.killpg(process.pid, signal.SIGKILL)
                    except ProcessLookupError:
                        pass
            shutil.rmtree(profile_root, ignore_errors=True)

    def atspi(self) -> str:
        output = self.command(["gdbus", "introspect", "--session", "--dest", "org.a11y.Bus", "--object-path", "/org/a11y/bus"])
        if "GetAddress" not in output:
            raise RuntimeError("AT-SPI bus has no GetAddress method")
        return "AT-SPI bus exported"

    def run(self) -> int:
        self.evidence.mkdir(parents=True, exist_ok=True)
        self.case("session", self.session)
        self.case("network-isolation", self.network_isolation)
        self.case("portal", self.portal)
        self.case("tray", self.tray)
        self.case("audio-loopback", self.audio)
        self.case("capture", self.capture)
        self.case("browsers", self.browsers)
        self.case("at-spi", self.atspi)
        value = {
            "schema_version": 1,
            "lane": self.lane,
            "tests": self.tests,
            "environment": {
                "session_type": os.environ.get("XDG_SESSION_TYPE", "unknown"),
                "display": "local-console",
                "probe_version": "1",
                "network_policy": "output-drop-v1",
            },
            "artifacts": [path.name for path in sorted(self.evidence.iterdir()) if path.is_file()],
        }
        self.output.parent.mkdir(parents=True, exist_ok=True)
        self.output.write_text(json.dumps(value, sort_keys=True, indent=2) + "\n", encoding="utf-8")
        return 0 if all(item["status"] == "pass" for item in self.tests) else 1


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--e2e-probe", action="store_true")
    parser.add_argument("--lane", choices=("linux-wayland", "linux-x11"), required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--evidence", type=Path, required=True)
    args = parser.parse_args()
    return Probe(args.lane, args.output, args.evidence).run()


if __name__ == "__main__":
    raise SystemExit(main())
