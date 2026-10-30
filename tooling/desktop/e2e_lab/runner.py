"""Build and execute lane artifacts while collecting strict evidence."""

from __future__ import annotations

import json
import os
import secrets
import shutil
import stat
from collections import Counter
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from artifacts import SourceFingerprint
from browser_probe import build as build_browser_probe
from build_cache import restore as restore_build_cache, store as store_build_cache
from config import LabConfig
from errors import IntegrityError, LabError
from limits import enforce_run_limits
from linux_artifacts import build_current_linux_artifact, inspect_linux_artifact
from paths import LabPaths
from progress import emit as emit_progress
from receipt import redact_data, redact_text
from results import GuestResult, parse_guest_result
from ssh import GuestSsh, powershell_encoded
from system import sha256_file
from vm import VmManager

LINUX_CASES = {
    "session",
    "network-isolation",
    "portal",
    "tray",
    "audio-loopback",
    "capture",
    "browsers",
    "at-spi",
    "package-lifecycle",
}
TEXT_EVIDENCE_SUFFIXES = {".json", ".jsonl", ".log", ".txt", ".xml", ".html", ".csv"}
BINARY_EVIDENCE_SUFFIXES = {".png", ".jpg", ".jpeg", ".raw", ".wav", ".mp4", ".webm"}
MAX_TEXT_EVIDENCE_BYTES = 4 * 1024 * 1024
WINDOWS_CASES = {
    "network-isolation",
    "install",
    "upgrade",
    "repair",
    "resident",
    "tray",
    "settings",
    "native-messaging",
    "capture",
    "recording-system-audio",
    "recording-microphone",
    "reconnect-recovery",
    "uninstall",
}
WINDOWS_CASE_DEPENDENCIES = {
    "install": {"network-isolation"},
    "upgrade": {"install"},
    "repair": {"upgrade"},
    "resident": {"repair"},
    "tray": {"resident"},
    "settings": {"tray"},
    "native-messaging": {"resident"},
    "capture": {"tray"},
    "recording-system-audio": {"resident"},
    "recording-microphone": {"resident"},
    "reconnect-recovery": {"native-messaging"},
    "uninstall": {"resident"},
}


def expand_windows_cases(requested: set[str] | None) -> set[str]:
    if requested is None:
        return set(WINDOWS_CASES)
    unknown = requested - WINDOWS_CASES
    if unknown:
        raise LabError("unknown Windows E2E case(s): " + ", ".join(sorted(unknown)))
    expanded = set(requested)
    if requested:
        expanded.add("uninstall")
    pending = list(expanded)
    while pending:
        name = pending.pop()
        for dependency in WINDOWS_CASE_DEPENDENCIES.get(name, set()):
            if dependency not in expanded:
                expanded.add(dependency)
                pending.append(dependency)
    return expanded


@dataclass(frozen=True)
class WindowsTestArtifacts:
    current_msi: Path
    previous_msi: Path
    baseline_exe: Path
    built_exe: Path
    native_manifest: Path
    profile: str


class LaneRunner:
    def __init__(self, paths: LabPaths, config: LabConfig) -> None:
        self.paths = paths
        self.config = config
        self.vms = VmManager(paths, config)
        self.teardowns: dict[str, dict[str, bool]] = {}
        self.consumed_artifacts: list[Path] = []
        self.build_caches: dict[str, str] = {}
        self.work_session = False
        self.real_extension: Path | None = None
        self.vendor_bundle: Path | None = None

    def _record_teardown(self, lane: str, phase: str, pre_forced: bool, post_forced: bool) -> None:
        value = self.teardowns.setdefault(lane, {})
        value[f"{phase}_pre_forced"] = pre_forced
        value[f"{phase}_post_forced"] = post_forced
        value["pre_run_forced"] = value.get("pre_run_forced", False) or pre_forced
        value["post_run_forced"] = value.get("post_run_forced", False) or post_forced

    def _request_guest_shutdown(self, lane_name: str) -> None:
        if self.vms.current(lane_name, clean_stale=False) is None:
            return
        lane = self.config.lanes[lane_name]
        ssh = GuestSsh(self.paths, lane.ssh_port, "lab")
        command = ["sudo", "systemctl", "poweroff"] if lane.os == "linux" else ["shutdown.exe", "/s", "/t", "0", "/f"]
        try:
            ssh.command(command, timeout=20)
        except LabError:
            # A successful shutdown commonly closes SSH before it can return a
            # status. VmManager still verifies exit and enforces its timeout.
            pass

    def _finish_vm(self, lane_name: str, phase: str, pre_forced: bool) -> None:
        if self.work_session:
            self.vms.assert_running(lane_name)
            return
        graceful_error: Exception | None = None
        try:
            self._request_guest_shutdown(lane_name)
        except (IntegrityError, LabError, OSError) as exc:
            graceful_error = exc
        try:
            post_forced = self.vms.reset(lane_name)
        except (IntegrityError, LabError, OSError) as exc:
            self._record_teardown(lane_name, phase, pre_forced, True)
            raise LabError(f"{lane_name} reset failed after {phase}: {exc}") from exc
        self._record_teardown(lane_name, phase, pre_forced, post_forced)
        if graceful_error is not None:
            raise LabError(
                f"{lane_name} graceful shutdown inspection failed after {phase}; reset completed: {graceful_error}"
            ) from graceful_error

    def build(self, lane_name: str, profile: str, source: SourceFingerprint, run_dir: Path) -> list[Path]:
        lane = self.config.lanes[lane_name]
        output = run_dir / "artifacts" / lane_name
        output.mkdir(parents=True, exist_ok=True, mode=0o700)
        if lane.os == "linux":
            artifacts = build_current_linux_artifact(
                self.paths.root,
                profile=profile,
                source_archive=source.archive,
                output=output,
            )
            self._record_teardown(lane_name, "build", False, False)
            return artifacts
        cached = restore_build_cache(self.paths, lane_name, profile, source.sha256, output)
        if cached is not None:
            self.build_caches[lane_name] = "hit"
            emit_progress(
                "build-cache",
                lane=lane_name,
                profile=profile,
                status="hit",
                sourceSha256=source.sha256,
            )
            (output / "build.log").write_text(f"content-addressed build cache hit: {source.sha256}\n", encoding="utf-8")
            self._record_teardown(lane_name, "build", False, False)
            return cached
        emit_progress(
            "build-cache",
            lane=lane_name,
            profile=profile,
            status="miss",
            sourceSha256=source.sha256,
        )
        built = self._build_windows(lane_name, profile, source, output)
        store_build_cache(self.paths, lane_name, profile, source.sha256, output)
        self.build_caches[lane_name] = "miss-stored"
        emit_progress(
            "build-cache",
            lane=lane_name,
            profile=profile,
            status="stored",
            sourceSha256=source.sha256,
        )
        return built

    def _build_windows(self, lane_name: str, profile: str, source: SourceFingerprint, output: Path) -> list[Path]:
        pre_forced = False if self.work_session else self.vms.reset(lane_name)
        try:
            self.vms.start(
                lane_name, allow_guest_network=not self.work_session, windows_audio_capture=self.work_session
            )
            ssh = GuestSsh(self.paths, self.config.lanes[lane_name].ssh_port, "lab")
            ssh.wait(
                self.config.timeouts.boot,
                probe=["cmd.exe", "/d", "/c", "exit", "0"],
                health_check=lambda: self.vms.assert_running(lane_name),
            )
            if not self.work_session:
                from windows_session import require_license

                require_license(ssh, 7200)
            self._assert_windows_audio_backend(lane_name)
            ssh.upload(source.archive, "source.tar.gz", timeout=600)
            script = self.paths.root / "tooling/desktop/e2e_lab/guest/windows/build.ps1"
            ssh.upload(script, "build.ps1", timeout=120)
            command = [
                "powershell.exe",
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
                "build.ps1",
                "-SourceBundle",
                "source.tar.gz",
                "-SourceSha256",
                source.sha256,
                "-Profile",
                profile,
                "-Output",
                "out",
            ]
            if self.work_session:
                from session_dependencies import upload_wix_ui
                from session_toolchain import upload_toolchain

                compiler = upload_toolchain(self.paths, ssh)
                if self.vendor_bundle is not None:
                    ssh.upload(self.vendor_bundle, "vendor.tar.gz", timeout=300)
                    command.extend(("-VendorBundle", "vendor.tar.gz", "-VendorSha256", sha256_file(self.vendor_bundle)))
                wix_hash = upload_wix_ui(self.paths, ssh, output)
                command.extend(
                    (
                        "-WorkspaceMode",
                        "warm",
                        "-WixUiSha256",
                        wix_hash,
                        "-RustcSha256",
                        compiler["rustc_sha256"],
                        "-CargoSha256",
                        compiler["cargo_sha256"],
                    )
                )
            ssh.command(
                command,
                timeout=self.config.timeouts.test,
                log=output / "build.log",
                stream=True,
            )
            self._enforce_remote_tree_limit(ssh, "out", windows=True, max_bytes=1024 * 1024 * 1024, max_files=512)
            ssh.download(
                "out/",
                output,
                recursive=True,
                timeout=600,
                max_bytes=(1024 + self.config.limits.max_log_mib) * 1024 * 1024,
                max_files=513,
            )
        finally:
            self._finish_vm(lane_name, "build", pre_forced)
        artifacts = sorted(
            path for path in output.rglob("*") if path.is_file() and path.suffix.lower() in {".exe", ".msi"}
        )
        if not artifacts:
            raise LabError("Windows guest build produced no EXE/MSI artifacts")
        return artifacts

    def test(
        self,
        lane_name: str,
        artifact: Path,
        run_dir: Path,
        requested_cases: set[str] | None = None,
    ) -> GuestResult:
        if not os.access("/dev/kvm", os.R_OK | os.W_OK):
            raise LabError("desktop E2E acceptance requires readable and writable /dev/kvm")
        lane = self.config.lanes[lane_name]
        if lane.os == "linux":
            return self._test_linux(lane_name, artifact, run_dir)
        if (artifact.parent / "provenance.json").is_file():
            if requested_cases is not None:
                raise LabError("alpha release acceptance cannot select or skip cases")
            from release_acceptance import test_release

            return test_release(self, lane_name, artifact, run_dir)
        return self._test_windows(lane_name, artifact, run_dir, requested_cases)

    def _test_linux(self, lane_name: str, artifact: Path, run_dir: Path) -> GuestResult:
        plan = inspect_linux_artifact(artifact)
        pre_forced = self.vms.reset(lane_name)
        lane_dir = run_dir / "guest" / lane_name
        lane_dir.mkdir(parents=True, exist_ok=True, mode=0o700)
        try:
            self.vms.start(
                lane_name,
                windows_audio_capture=microphone_required,
                host_reserve_mib=1024,
            )
            ssh = GuestSsh(self.paths, self.config.lanes[lane_name].ssh_port, "lab")
            ssh.wait(self.config.timeouts.boot, health_check=lambda: self.vms.assert_running(lane_name))
            lockdown = self.paths.root / "tooling/desktop/e2e_lab/guest/linux-network-lockdown.sh"
            ssh.upload(lockdown, "linux-network-lockdown.sh", timeout=120)
            ssh.command(["sudo", "bash", "linux-network-lockdown.sh"], timeout=120)
            ssh.upload(artifact, plan.remote_name, timeout=600)
            session_exec = self.paths.root / "tooling/desktop/e2e_lab/guest/linux-session-exec.sh"
            ssh.upload(session_exec, "sniptale-session-exec", timeout=120)
            ssh.command(
                ["sudo", "install", "-m", "0755", "sniptale-session-exec", "/usr/local/bin/sniptale-session-exec"],
                timeout=120,
            )
            script = self.paths.root / "tooling/desktop/e2e_lab/guest/linux-run.sh"
            ssh.upload(script, "linux-run.sh", timeout=120)
            try:
                ssh.command(
                    [
                        "bash",
                        "linux-run.sh",
                        lane_name,
                        plan.remote_name,
                        sha256_file(artifact),
                        plan.kind,
                        plan.package_name,
                        plan.executable,
                    ],
                    timeout=self.config.timeouts.test,
                    log=run_dir / "logs" / f"{lane_name}.log",
                )
            except LabError:
                pass
            self._enforce_remote_evidence_limit(ssh, windows=False)
            maximum = self._evidence_transfer_maximum()
            ssh.download("e2e-out/", lane_dir, recursive=True, timeout=600, max_bytes=maximum, max_files=2048)
        finally:
            self._collect_qemu_log(lane_name, run_dir)
            self._finish_vm(lane_name, "test", pre_forced)
        self._validate_guest_tree(lane_dir)
        self._sanitize_guest_text_tree(lane_dir)
        result_path = next(iter(lane_dir.rglob("result.json")), None)
        if result_path is None:
            raise LabError(f"{lane_name} did not return result.json")
        self._classify_evidence(lane_dir, run_dir)
        enforce_run_limits(run_dir, self.config.limits)
        result = parse_guest_result(result_path, expected_lane=lane_name, required_cases=LINUX_CASES)
        self._verify_declared_artifacts(lane_dir, result)
        return result

    def _collect_qemu_log(self, lane_name: str, run_dir: Path) -> None:
        """Keep a bounded, redacted backend log before disposable runtime teardown."""
        source = self.paths.runtime / lane_name / "qemu.log"
        if not source.is_file() or source.is_symlink():
            return
        size = source.stat().st_size
        with source.open("rb") as stream:
            if size <= MAX_TEXT_EVIDENCE_BYTES:
                payload = stream.read(MAX_TEXT_EVIDENCE_BYTES + 1)
            else:
                half = (MAX_TEXT_EVIDENCE_BYTES - 64) // 2
                head = stream.read(half)
                stream.seek(max(0, size - half))
                tail = stream.read(half)
                payload = head + b"\n[qemu-log-truncated]\n" + tail
        target = run_dir / "logs" / f"{lane_name}-qemu.log"
        target.write_text(
            redact_text(payload.decode("utf-8", errors="replace")),
            encoding="utf-8",
        )
        target.chmod(0o600)

    def _test_windows(
        self,
        lane_name: str,
        artifact: Path,
        run_dir: Path,
        requested_cases: set[str] | None = None,
    ) -> GuestResult:
        artifact_set = self._windows_test_artifacts(artifact)
        real_extension_requested = (
            self.work_session
            and self.real_extension is not None
            and (requested_cases is None or "real-extension" in requested_cases)
        )
        runtime_requested = (
            requested_cases - {"real-extension"}
            if self.work_session and requested_cases is not None
            else requested_cases
        )
        if real_extension_requested and runtime_requested is not None:
            runtime_requested = runtime_requested | {"resident"}
        selected_cases = expand_windows_cases(runtime_requested)
        if real_extension_requested:
            selected_cases.add("real-extension")
        if self.work_session:
            selected_cases -= {"install", "upgrade", "repair", "uninstall"}
        microphone_required = "recording-microphone" in selected_cases
        pre_forced = False if self.work_session else self.vms.reset(lane_name)
        lane_dir = run_dir / "guest" / lane_name
        lane_dir.mkdir(parents=True, exist_ok=True, mode=0o700)
        try:
            self.vms.start(
                lane_name,
                windows_audio_capture=microphone_required or self.work_session,
                host_reserve_mib=1024,
            )
            ssh = GuestSsh(self.paths, self.config.lanes[lane_name].ssh_port, "lab")
            ssh.wait(
                self.config.timeouts.boot,
                probe=["cmd.exe", "/d", "/c", "exit", "0"],
                health_check=lambda: self.vms.assert_running(lane_name),
            )
            if not self.work_session:
                from windows_session import require_license

                require_license(ssh, 7200)
            self._assert_windows_audio_backend(
                lane_name,
                require_capture=microphone_required,
            )
            current_remote_name = artifact_set.current_msi.name
            profile = artifact_set.profile
            previous_msi = artifact_set.previous_msi
            previous_remote_name = previous_msi.name
            baseline_exe = artifact_set.baseline_exe
            built_exe = artifact_set.built_exe
            native_manifest = artifact_set.native_manifest
            self.consumed_artifacts.extend((previous_msi, baseline_exe, built_exe, native_manifest))
            expected_exe_sha256 = sha256_file(built_exe)
            previous_exe_sha256 = sha256_file(baseline_exe)
            probe_nonce = secrets.token_hex(32)
            if real_extension_requested:
                from browser_automation import prepare

                prepare(self.paths, ssh)
            browser_probe = build_browser_probe(self.paths.root, run_dir / ".browser-probe.zip", nonce=probe_nonce)
            browser_probe_sha256 = sha256_file(browser_probe)
            script = self.paths.root / "tooling/desktop/e2e_lab/guest/windows/run.ps1"
            identity_verifier = self.paths.root / "tooling/release/windows/verify_msi_identity.ps1"
            launcher = script.with_name("launch-interactive.ps1")
            uploads = {
                current_remote_name: artifact_set.current_msi,
                previous_remote_name: previous_msi,
                "browser-probe.zip": browser_probe,
                "run.ps1": script,
                "windows-ui.ps1": script.with_name("windows-ui.ps1"),
                "verify_msi_identity.ps1": identity_verifier,
                "launch-interactive.ps1": launcher,
            }
            certificate = artifact.parent / "lab-signing.cer"
            if certificate.is_file():
                if certificate.is_symlink() or certificate.stat().st_size > 16384:
                    raise IntegrityError("invalid laboratory certificate artifact")
                uploads["lab-signing.cer"] = certificate
                uploads["lab-signing.ps1"] = script.with_name("lab-signing.ps1")
                self.consumed_artifacts.append(certificate)
            if real_extension_requested:
                uploads["real-extension.zip"] = self.real_extension
                for name in (
                    "real-extension.ps1",
                    "browser-playwright.ps1",
                    "browser-playwright.cjs",
                    "real-extension.js",
                ):
                    uploads[name] = script.with_name(name)
            try:
                ssh.upload_many(uploads, timeout=600)
            finally:
                browser_probe.unlink(missing_ok=True)
            if certificate.is_file():
                # Trust preparation uses the SSH administrator context; the UI task stays limited.
                # Prove denied egress before changing any trust or installer state.
                digest = sha256_file(certificate)
                ssh.command(
                    powershell_encoded(
                        "$ErrorActionPreference='Stop'; $ProgressPreference='SilentlyContinue'; "
                        "$client=[Net.Sockets.TcpClient]::new(); try {$attempt=$client.ConnectAsync('1.1.1.1',443); "
                        "try {$null=$attempt.Wait(3000)} catch [AggregateException] {}; "
                        "if($client.Connected){throw 'runtime egress must be denied before laboratory trust'}} finally {$client.Dispose()}; "
                        ". ./lab-signing.ps1; "
                        f"$null=Import-LabSigningIdentity (Join-Path $PWD 'lab-signing.cer') '{digest}'; "
                        "Write-Output 'isolated-laboratory-certificate-ready'",
                        allow_lab_scripts=True,
                    ),
                    timeout=60,
                    log=run_dir / "logs/laboratory-trust.log",
                )
            remote_command = [
                "powershell.exe",
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
                "launch-interactive.ps1",
                "-Runner",
                "run.ps1",
                "-Msi",
                current_remote_name,
                "-MsiSha256",
                sha256_file(artifact),
                "-PreviousMsi",
                previous_remote_name,
                "-PreviousMsiSha256",
                sha256_file(previous_msi),
                "-PreviousExeSha256",
                previous_exe_sha256,
                "-ExpectedExeSha256",
                expected_exe_sha256,
                "-ExpectedNativeHostManifestSha256",
                sha256_file(native_manifest),
                "-IdentityVerifier",
                "verify_msi_identity.ps1",
                "-IdentityVerifierSha256",
                sha256_file(identity_verifier),
                "-BrowserProbe",
                "browser-probe.zip",
                "-BrowserProbeSha256",
                browser_probe_sha256,
                "-ProbeNonce",
                probe_nonce,
                "-Output",
                "e2e-out",
                "-TimeoutSeconds",
                str(self.config.timeouts.test),
            ]
            if certificate.is_file():
                remote_command.extend(("-LabCertificateSha256", sha256_file(certificate)))
            if requested_cases is not None or self.work_session:
                remote_command.extend(("-Cases", ",".join(sorted(selected_cases))))
            if self.work_session:
                remote_command.extend(("-WorkspaceMode", "warm"))
            if real_extension_requested:
                remote_command.extend(("-RealExtensionSha256", sha256_file(self.real_extension)))
            runner_error: LabError | None = None
            try:
                ssh.command(
                    remote_command,
                    timeout=self.config.timeouts.test,
                    log=run_dir / "logs" / f"{lane_name}.log",
                    stream=True,
                )
            except LabError as exc:
                runner_error = exc
            self._assert_windows_audio_backend(
                lane_name,
                require_capture=microphone_required,
            )
            try:
                self._enforce_remote_evidence_limit(ssh, windows=True)
            except LabError:
                if runner_error is not None:
                    raise runner_error
                raise
            maximum = self._evidence_transfer_maximum()
            ssh.download("e2e-out/", lane_dir, recursive=True, timeout=600, max_bytes=maximum, max_files=2048)
        finally:
            self._collect_qemu_log(lane_name, run_dir)
            self._finish_vm(lane_name, "test", pre_forced)
        self._validate_guest_tree(lane_dir)
        self._sanitize_guest_text_tree(lane_dir)
        result_path = next(iter(lane_dir.rglob("result.json")), None)
        if result_path is None:
            if runner_error is not None:
                raise runner_error
            raise LabError("Windows guest did not return result.json")
        required = selected_cases
        self._classify_evidence(lane_dir, run_dir)
        enforce_run_limits(run_dir, self.config.limits)
        result = parse_guest_result(result_path, expected_lane=lane_name, required_cases=required)
        self._verify_declared_artifacts(lane_dir, result)
        return result

    def _assert_windows_audio_backend(
        self,
        lane_name: str,
        *,
        require_capture: bool = True,
    ) -> None:
        """Fail fast when QEMU could not bind a guest render/capture voice."""
        log_path = self.paths.runtime / lane_name / "qemu.log"
        if not log_path.is_file():
            return
        # QEMU may repeat one backend error for every attempted guest voice. Bound the read while
        # retaining both the first configuration errors and the latest device-open failures.
        size = log_path.stat().st_size
        with log_path.open("rb") as stream:
            head = stream.read(min(size, 1024 * 1024))
            tail = b""
            if size > 1024 * 1024:
                stream.seek(max(0, size - 1024 * 1024))
                tail = stream.read(1024 * 1024)
        text = (head + tail).decode("utf-8", errors="replace")
        failures = (
            "Could not initialize DAC",
            "Failed to open `",
        )
        matched = {failure for failure in failures if failure in text}
        if "Could not create a backend for voice `dac'" in text:
            matched.add("Could not create a backend for voice `dac'")
        if require_capture:
            for failure in (
                "Could not initialize ADC",
                "Could not create a backend for voice `adc'",
            ):
                if failure in text:
                    matched.add(failure)
        if matched:
            raise LabError("QEMU audio backend is unhealthy: " + ", ".join(sorted(matched)))

    @staticmethod
    def _windows_test_artifacts(artifact: Path) -> WindowsTestArtifacts:
        current = artifact.resolve(strict=True)
        if current.suffix.lower() != ".msi":
            raise LabError("Windows E2E requires an MSI artifact")
        if "0.1.0" not in current.name:
            raise LabError("Windows E2E current MSI must identify version 0.1.0 in its filename")
        profile = (
            "release" if current.name.endswith("-release.msi") else "dev" if current.name.endswith("-dev.msi") else ""
        )
        if not profile:
            raise LabError("Windows E2E MSI filename must identify its dev or release profile")
        previous = next(iter(sorted(current.parent.glob(f"*0.0.1-{profile}.msi"))), None)
        if previous is None or previous.resolve() == current:
            raise LabError("Windows E2E requires a distinct sibling 0.0.1 baseline MSI")
        baseline_exe = current.parent / f"sniptale-native-agent-baseline-{profile}.exe"
        built_exe = current.parent / f"sniptale-native-agent-{profile}.exe"
        native_manifest = current.parent / "native-host.json"
        if not baseline_exe.is_file() or not built_exe.is_file() or not native_manifest.is_file():
            raise LabError("Windows E2E requires sibling baseline/current EXE and native-host.json proof artifacts")
        return WindowsTestArtifacts(
            current_msi=current,
            previous_msi=previous.resolve(strict=True),
            baseline_exe=baseline_exe.resolve(strict=True),
            built_exe=built_exe.resolve(strict=True),
            native_manifest=native_manifest.resolve(strict=True),
            profile=profile,
        )

    def _enforce_remote_evidence_limit(self, ssh: GuestSsh, *, windows: bool) -> None:
        self._enforce_remote_tree_limit(
            ssh,
            "e2e-out",
            windows=windows,
            max_bytes=self._evidence_transfer_maximum(),
            max_files=2048,
        )

    def _evidence_transfer_maximum(self) -> int:
        limits = self.config.limits
        return (limits.max_log_mib + limits.max_screenshot_mib + limits.max_recording_mib + 16) * 1024 * 1024

    @staticmethod
    def _enforce_remote_tree_limit(
        ssh: GuestSsh,
        remote: str,
        *,
        windows: bool,
        max_bytes: int,
        max_files: int,
    ) -> None:
        if windows:
            command = powershell_encoded(
                "$ProgressPreference='SilentlyContinue'; "
                f"$items=@(Get-ChildItem -LiteralPath '{remote}' -Recurse -Force); "
                "$bad=@($items | Where-Object { $_.Attributes -band [IO.FileAttributes]::ReparsePoint }); "
                "if($bad.Count -gt 0){throw 'guest evidence contains a reparse point'}; "
                "$files=@($items | Where-Object {-not $_.PSIsContainer}); "
                "$n=($files | Measure-Object -Property Length -Sum).Sum; "
                'if($null -eq $n){$n=0}; Write-Output "$($files.Count) $n"'
            )
        else:
            unsafe = ssh.command(
                ["find", remote, "-xdev", "!", "-type", "d", "!", "-type", "f", "-print", "-quit"],
                timeout=60,
            ).stdout.strip()
            if unsafe:
                raise LabError("guest evidence contains an unsafe file type or link")
            command = [
                "bash",
                "-lc",
                f"set -euo pipefail; printf '%s ' \"$(find {remote} -xdev -type f -printf . | wc -c)\"; du -sb {remote} | cut -f1",
            ]
        output = ssh.command(command, timeout=60).stdout.strip().split()
        try:
            count, size = (int(output[-2]), int(output[-1]))
        except (IndexError, ValueError) as exc:
            raise LabError("guest evidence size report is invalid") from exc
        if count > max_files or size > max_bytes:
            raise LabError("guest tree exceeds its transfer byte or file-count limit")

    @staticmethod
    def _validate_guest_tree(source: Path) -> None:
        root = source.resolve()
        file_count = 0
        for current, directories, files in os.walk(source, topdown=True, followlinks=False):
            current_path = Path(current)
            for name in directories:
                path = current_path / name
                if path.is_symlink() or not path.resolve(strict=False).is_relative_to(root):
                    raise LabError("guest evidence contains an unsafe directory link")
            for name in files:
                file_count += 1
                if file_count > 2048:
                    raise LabError("guest evidence exceeds the local file-count limit")
                path = current_path / name
                mode = path.lstat().st_mode
                if not stat.S_ISREG(mode) or not path.resolve(strict=False).is_relative_to(root):
                    raise LabError("guest evidence contains an unsafe file type or link")

    @staticmethod
    def _sanitize_guest_text_tree(source: Path) -> None:
        first_error: LabError | None = None
        for path in sorted(source.rglob("*")):
            if not path.is_file():
                continue
            suffix = path.suffix.lower()
            if suffix in BINARY_EVIDENCE_SUFFIXES:
                continue
            if suffix not in TEXT_EVIDENCE_SUFFIXES:
                path.unlink(missing_ok=True)
                first_error = first_error or LabError("guest evidence contains an unsupported file type")
                continue
            descriptor: int | None = None
            try:
                descriptor = os.open(path, os.O_RDONLY | os.O_CLOEXEC | os.O_NOFOLLOW)
                metadata = os.fstat(descriptor)
                if not stat.S_ISREG(metadata.st_mode) or metadata.st_size > MAX_TEXT_EVIDENCE_BYTES:
                    raise LabError("guest textual evidence exceeds its per-file limit")
                with os.fdopen(descriptor, "r", encoding="utf-8", errors="strict") as stream:
                    descriptor = None
                    payload = stream.read()
                if suffix == ".json":
                    if path.name == "result.json":
                        sanitized_value = LaneRunner._sanitize_guest_result(json.loads(payload))
                    elif path.name == "real-extension-result.json":
                        from session_evidence import sanitize_real_extension

                        sanitized_value = sanitize_real_extension(json.loads(payload))
                    else:
                        raise LabError("guest evidence contains an unsupported JSON file")
                    sanitized = json.dumps(sanitized_value, sort_keys=True, indent=2) + "\n"
                elif suffix == ".jsonl":
                    lines = payload.splitlines()
                    if len(lines) > 10_000:
                        raise LabError("guest JSONL evidence exceeds its line limit")
                    sanitized_lines = []
                    for line in lines:
                        value = json.loads(line)
                        if not isinstance(value, dict):
                            raise LabError("guest JSONL record is not an object")
                        sanitized_lines.append(json.dumps(redact_data(value), sort_keys=True))
                    sanitized = "\n".join(sanitized_lines) + ("\n" if sanitized_lines else "")
                else:
                    sanitized = redact_text(payload)
                path.write_text(sanitized, encoding="utf-8")
            except UnicodeError:
                path.unlink(missing_ok=True)
                first_error = first_error or LabError("guest textual evidence is not valid UTF-8")
            except json.JSONDecodeError:
                path.unlink(missing_ok=True)
                first_error = first_error or LabError("guest result JSON cannot be safely sanitized")
            except OSError:
                path.unlink(missing_ok=True)
                first_error = first_error or LabError("guest textual evidence is unreadable")
            except LabError as exc:
                path.unlink(missing_ok=True)
                first_error = first_error or exc
            finally:
                if descriptor is not None:
                    os.close(descriptor)
        if first_error is not None:
            raise first_error

    @staticmethod
    def _sanitize_guest_result(value: Any) -> dict[str, Any]:
        if not isinstance(value, dict) or value.get("schema_version") != 1:
            raise LabError("guest result schema is invalid")
        lane = value.get("lane")
        if lane not in {"windows-11", "linux-wayland", "linux-x11"}:
            raise LabError("guest result lane is invalid")
        tests = value.get("tests")
        if not isinstance(tests, list) or not tests or len(tests) > 256:
            raise LabError("guest result tests are invalid")
        sanitized_tests: list[dict[str, Any]] = []
        for case in tests:
            if not isinstance(case, dict):
                raise LabError("guest result test is invalid")
            name = case.get("name")
            status = case.get("status")
            duration = case.get("duration_ms")
            detail = case.get("detail", "")
            if (
                not isinstance(name, str)
                or not name
                or len(name) > 128
                or status not in {"pass", "fail", "skip"}
                or not isinstance(duration, int)
                or isinstance(duration, bool)
                or duration < 0
                or not isinstance(detail, str)
            ):
                raise LabError("guest result test fields are invalid")
            sanitized_tests.append(
                {
                    "name": name,
                    "status": status,
                    "duration_ms": duration,
                    "detail": redact_text(detail, max_length=4096),
                }
            )
        environment = value.get("environment")
        artifacts = value.get("artifacts", [])
        if (
            not isinstance(environment, dict)
            or not isinstance(artifacts, list)
            or len(artifacts) > 256
            or not all(isinstance(item, str) and 0 < len(item) <= 255 and Path(item).name == item for item in artifacts)
        ):
            raise LabError("guest result environment or artifact inventory is invalid")
        return {
            "schema_version": 1,
            "lane": lane,
            "tests": sanitized_tests,
            "environment": redact_data(environment),
            "artifacts": artifacts,
        }

    @staticmethod
    def _classify_evidence(source: Path, run_dir: Path) -> None:
        for path in source.rglob("*"):
            if not path.is_file():
                continue
            relative_name = "-".join(path.relative_to(source).parts)
            evidence_name = f"{source.name}-{relative_name}"
            suffix = path.suffix.lower()
            if suffix in {".png", ".jpg", ".jpeg"}:
                destination = run_dir / "screenshots" / evidence_name
            elif suffix in {".raw", ".wav", ".mp4", ".webm"}:
                destination = run_dir / "recordings" / evidence_name
            elif suffix == ".log":
                destination = run_dir / "logs" / evidence_name
            else:
                continue
            shutil.copy2(path, destination)

    @staticmethod
    def _verify_declared_artifacts(source: Path, result: GuestResult) -> None:
        actual = Counter(path.name for path in source.rglob("*") if path.is_file() and path.name != "result.json")
        declared = Counter(result.artifacts)
        if actual != declared:
            raise IntegrityError("guest result artifact inventory does not match transferred evidence")
