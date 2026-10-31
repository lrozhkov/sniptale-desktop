"""Bounded development VM sessions. Their evidence never admits a release."""

from __future__ import annotations

import contextlib
import re
import shutil
import subprocess
import sys
import tarfile
import time
from pathlib import Path
from typing import Any, Literal, TypedDict, cast

from artifacts import create_source_bundle
from errors import IntegrityError, LabError, UsageError
from errors import ResourceError
from limits import directory_allocated_bytes
from progress import emit
from receipt import create_run_dir
from results import write_junit
from runner import LaneRunner
from ssh import GuestSsh, powershell_encoded
from state import process_matches, read_json, write_json
from system import run, sha256_file
from vm import VmManager
from windows_session import prepare_console, require_license

LANE = "windows-11"
MAX_ITERATIONS = 50
MAX_TTL_SECONDS = 14400


class SessionState(TypedDict):
    schema_version: Literal[1]
    id: str
    pid: int
    started_epoch: int
    expires_epoch: int
    golden_sha256: str
    boot_id: str
    iterations: int
    vendor_lock: str
    last_artifact: str | None
    health: Literal["ready", "failed"]


def vendor_metadata(member):
    member.mtime = 0
    member.uid = member.gid = 0
    member.uname = member.gname = ""
    return member


def state_path(paths):
    return paths.scoped(paths.lab / "work-session.json")


def load_session(paths) -> SessionState | None:
    value = read_json(state_path(paths))
    if value is None:
        return None
    required = {
        "schema_version",
        "id",
        "pid",
        "started_epoch",
        "expires_epoch",
        "golden_sha256",
        "boot_id",
        "iterations",
        "vendor_lock",
        "last_artifact",
        "health",
    }
    if set(value) != required or type(value["schema_version"]) is not int or value["schema_version"] != 1:
        raise IntegrityError("invalid work session schema")
    if not isinstance(value["id"], str) or re.fullmatch(r"[0-9]{8}T[0-9]{6}Z(?:-[0-9]{3})?", value["id"]) is None:
        raise IntegrityError("invalid work session identity")
    for key in ("pid", "started_epoch", "expires_epoch", "iterations"):
        if type(value[key]) is not int or value[key] < (2 if key == "pid" else 0):
            raise IntegrityError("invalid work session numeric field")
    if (
        not 0 < value["expires_epoch"] - value["started_epoch"] <= MAX_TTL_SECONDS
        or value["iterations"] > MAX_ITERATIONS
    ):
        raise IntegrityError("invalid work session lifetime")
    for key in ("golden_sha256", "vendor_lock"):
        if not isinstance(value[key], str) or re.fullmatch(r"[0-9a-f]{64}", value[key]) is None:
            raise IntegrityError("invalid work session fingerprint")
    if (
        not isinstance(value["boot_id"], str)
        or re.fullmatch(r"[0-9]{1,19}", value["boot_id"]) is None
        or int(value["boot_id"]) == 0
    ):
        raise IntegrityError("invalid work session boot identity")
    if value["health"] not in ("ready", "failed") or (
        value["last_artifact"] is not None and not isinstance(value["last_artifact"], str)
    ):
        raise IntegrityError("invalid work session state")
    if value["last_artifact"] is not None:
        artifact = paths.scoped(paths.lab / value["last_artifact"])
        if not artifact.is_relative_to(paths.lab / "sessions" / value["id"]):
            raise IntegrityError("work session artifact escaped its session")
    return cast(SessionState, value)


@contextlib.contextmanager
def phase(timings, name):
    started = time.monotonic()
    emit(name, status="started", mode="development-session")
    status = "failed"
    try:
        yield
        status = "completed"
    finally:
        timings[name] = round((time.monotonic() - started) * 1000)
        emit(name, status=status, durationMs=timings[name], mode="development-session")


class WorkSessions:
    def __init__(self, paths, config):
        self.paths, self.config = paths, config
        self.vms = VmManager(paths, config)
        self.ssh = GuestSsh(paths, config.lanes[LANE].ssh_port, "lab")

    def boot_id(self):
        boot = ""
        for attempt in range(3):
            self.vms.assert_running(LANE)
            try:
                boot = self.ssh.command(
                    powershell_encoded(
                        "$ProgressPreference='SilentlyContinue'; $ErrorActionPreference='Stop'; "
                        "(Get-CimInstance Win32_OperatingSystem).LastBootUpTime.ToUniversalTime().Ticks"
                    ),
                    timeout=30,
                ).stdout.strip()
                break
            except LabError:
                if attempt == 2:
                    raise
        if re.fullmatch(r"[0-9]{1,19}", boot) is None or int(boot) == 0:
            raise IntegrityError("invalid guest boot identity")
        return boot

    def require(self, *, allow_failed=False):
        state = load_session(self.paths)
        if state is None:
            raise UsageError("open a development session first")
        if time.time() >= state["expires_epoch"]:
            self.close()
            raise UsageError("development session expired; open a fresh session")
        if directory_allocated_bytes(self.paths.lab) > self.config.limits.max_temporary_gib * 1024**3:
            self.close()
            raise ResourceError("development session exceeded temporary storage budget")
        vm = self.vms.current(LANE, clean_stale=False)
        if vm is None or (vm.pid, vm.started_epoch, vm.golden_sha256) != (
            state["pid"],
            state["started_epoch"],
            state["golden_sha256"],
        ):
            raise IntegrityError("development VM generation changed; close and reopen session")
        self.vms.assert_running(LANE)
        if self.boot_id() != state["boot_id"]:
            raise IntegrityError("development VM rebooted; close and reopen session")
        if state["health"] != "ready" and not allow_failed:
            raise UsageError("failed session requires recover or close before more work")
        return state

    def validate_toolchain(self):
        run(
            [sys.executable, str(self.paths.root / "tooling/qa/runtime/toolchain.py"), "--check"],
            cwd=self.paths.root,
            timeout=30,
        )

    def open(self, ttl_seconds=7200):
        self.validate_toolchain()
        if not 600 <= ttl_seconds <= MAX_TTL_SECONDS:
            raise UsageError("session lifetime must be 600..14400 seconds")
        if load_session(self.paths) is not None or self.vms.current(LANE, clean_stale=False) is not None:
            raise UsageError("close the existing session or VM first")
        timings = {}
        root = self.paths.scoped(self.paths.lab / "sessions")
        identity, directory = create_run_dir(root)
        vendor = directory / "vendor"
        try:
            with phase(timings, "dependencies"):
                run(
                    ["cargo", "vendor", "--locked", "--offline", "--versioned-dirs", str(vendor)],
                    cwd=self.paths.root,
                    timeout=300,
                    log=directory / "logs/vendor.log",
                )
                archive = directory / "vendor.tar.gz"
                with tarfile.open(archive, "w:gz", compresslevel=1) as bundle:
                    bundle.add(vendor, arcname="vendor", recursive=True, filter=vendor_metadata)
                if archive.stat().st_size > 512 * 1024 * 1024:
                    raise LabError("vendored dependencies exceed transfer budget")
                shutil.rmtree(self.paths.scoped(vendor))
            with phase(timings, "boot"):
                vm = self.vms.start(LANE, windows_audio_capture=True, memory_mib=6144, reuse_work_cache=True)
                self.ssh.wait(
                    self.config.timeouts.boot,
                    probe=["cmd.exe", "/d", "/c", "exit", "0"],
                    health_check=lambda: self.vms.assert_running(LANE),
                )
            with phase(timings, "license"):
                require_license(self.ssh, ttl_seconds)
            with phase(timings, "console"):
                prepare_console(self.ssh, self.config.timeouts.boot, health_check=lambda: self.vms.assert_running(LANE))
                boot = self.boot_id()
            with phase(timings, "dependencies-upload"):
                self.ssh.upload(archive, "vendor.tar.gz", timeout=300)
                digest = sha256_file(archive)
                self.ssh.command(
                    powershell_encoded(
                        "$ErrorActionPreference='Stop'; $ProgressPreference='SilentlyContinue'; "
                        f"if((Get-FileHash vendor.tar.gz).Hash.ToLowerInvariant() -ne '{digest}'){{throw 'vendor hash mismatch'}}; "
                        "New-Item -ItemType Directory -Force C:\\SniptaleLab | Out-Null; "
                        "Remove-Item C:\\SniptaleLab\\vendor -Recurse -Force -ErrorAction SilentlyContinue; "
                        "tar.exe -xzf vendor.tar.gz -C C:\\SniptaleLab; if($LASTEXITCODE -ne 0){throw 'vendor extraction failed'}; "
                        "Remove-Item vendor.tar.gz -Force"
                    ),
                    timeout=180,
                )
                archive.unlink()
            state = dict(
                schema_version=1,
                id=identity,
                pid=vm.pid,
                started_epoch=vm.started_epoch,
                expires_epoch=vm.started_epoch + ttl_seconds,
                golden_sha256=vm.golden_sha256,
                boot_id=boot,
                iterations=0,
                vendor_lock=sha256_file(self.paths.root / "Cargo.lock"),
                last_artifact=None,
                health="ready",
            )
            write_json(state_path(self.paths), state)
            write_json(
                directory / "session.json",
                {
                    **state,
                    "mode": "development-session",
                    "release_admissible": False,
                    "timings_ms": timings,
                    "work_image_cache": read_json(self.paths.runtime / LANE / "development-base.json"),
                },
            )
            subprocess.Popen(
                [sys.executable, str(Path(__file__).with_name("session_watchdog.py")), str(self.paths.root), identity],
                stdin=subprocess.DEVNULL,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                start_new_session=True,
                close_fds=True,
            )
            return {**state, "timings_ms": timings, "release_admissible": False}
        except BaseException:
            log = self.paths.runtime / LANE / "qemu.log"
            if log.is_file():
                with log.open("rb") as stream:
                    (directory / "logs/qemu.log").write_bytes(stream.read(1024 * 1024))
            self.vms.reset(LANE)
            state_path(self.paths).unlink(missing_ok=True)
            raise

    def save_work_image(self, vm, state):
        if state["health"] != "ready":
            raise UsageError("recover and complete a successful iteration before saving a work image")
        self.vms.assert_running(LANE)
        self.ssh.command(["shutdown.exe", "/s", "/t", "3", "/f"], timeout=30)
        deadline = time.monotonic() + max(180, self.config.timeouts.shutdown)
        while process_matches(vm.pid, "sniptale-lab-" + LANE, str(self.paths.lab)) and time.monotonic() < deadline:
            time.sleep(0.5)
        if process_matches(vm.pid, "sniptale-lab-" + LANE, str(self.paths.lab)):
            raise LabError("guest did not shut down cleanly; work image was not cached")
        overlay = self.paths.scoped(vm.overlay)
        write_json(self.paths.runtime / LANE / "work-image-save.json", self.save_stamp(state, overlay))
        return self.store_work_image(state, overlay)

    def save_stamp(self, state, overlay):
        return dict(
            schema_version=1,
            session_id=state["id"],
            pid=state["pid"],
            started_epoch=state["started_epoch"],
            golden_sha256=state["golden_sha256"],
            overlay_sha256=sha256_file(overlay),
        )

    def store_work_image(self, state, overlay):
        from session_cache import store

        qemu_img = shutil.which("qemu-img") or str(self.paths.cache / "host-tools/bin/qemu-img")
        return store(
            self.paths,
            qemu_img,
            self.paths.golden / LANE / "disk.qcow2",
            overlay,
            state["golden_sha256"],
            state["id"],
            self.config.limits.max_cache_gib,
        )

    def close(self, save=False):
        state = load_session(self.paths)
        # A stale lease must never destroy an unrelated VM generation.
        raw = read_json(self.vms.state_path(LANE))
        if (
            state is not None
            and raw is not None
            and (raw.get("pid"), raw.get("started_epoch")) != (state["pid"], state["started_epoch"])
        ):
            raise IntegrityError("refusing to close another VM generation")
        pending = read_json(self.paths.runtime / LANE / "work-image-save.json") if save else None
        if pending is not None:
            overlay = self.paths.runtime / LANE / "overlay.qcow2"
            with phase({}, "work-image-save-retry"):
                if (
                    state is None
                    or raw is None
                    or state["health"] != "ready"
                    or raw.get("golden_sha256") != state["golden_sha256"]
                    or overlay.is_symlink()
                    or not overlay.is_file()
                    or self.vms.orphan_pids(LANE)
                    or process_matches(state["pid"], "sniptale-lab-" + LANE, str(self.paths.lab))
                    or pending != self.save_stamp(state, self.paths.scoped(overlay))
                ):
                    raise IntegrityError("pending work image save identity changed or VM is running")
                self.store_work_image(state, overlay)
            self.vms.reset(LANE)
            state_path(self.paths).unlink(missing_ok=True)
            return {"closed": True, "session_id": state["id"], "cache_saved": True}
        vm = self.vms.current(LANE, clean_stale=not save)
        if state is not None and vm is not None:
            if (vm.pid, vm.started_epoch) != (state["pid"], state["started_epoch"]):
                raise IntegrityError("refusing to close another VM generation")
            if save:
                if state["health"] != "ready":
                    raise UsageError("recover and complete a successful iteration before saving a work image")
                # Preserve the current disk and lease if shutdown or cache creation fails.
                # A failed save must not silently discard the user's warmed environment.
                with phase({}, "work-image-save"):
                    self.save_work_image(vm, state)
                self.vms.reset(LANE)
                state_path(self.paths).unlink(missing_ok=True)
            else:
                self.vms.reset(LANE)
        state_path(self.paths).unlink(missing_ok=True)
        return {
            "closed": True,
            "session_id": state["id"] if state else None,
            "cache_saved": bool(save and state is not None and vm is not None),
        }

    def recover(self):
        state = self.require(allow_failed=True)
        self.ssh.command(
            powershell_encoded(
                "$ErrorActionPreference='Stop'; $ProgressPreference='SilentlyContinue'; "
                "if(Test-Path 'HKLM:\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Installer\\InProgress'){throw 'installer recovery requires a fresh session'}; "
                "Stop-ScheduledTask -TaskName SniptaleDesktopE2E -ErrorAction SilentlyContinue; "
                "Unregister-ScheduledTask -TaskName SniptaleDesktopE2E -Confirm:$false -ErrorAction SilentlyContinue; "
                "$workers=@(Get-CimInstance Win32_Process | Where-Object {$_.ProcessId -ne $PID -and "
                "$_.Name -eq 'powershell.exe' -and $_.CommandLine -match '(?i)-File\\s+\"?(?:C:\\\\Users\\\\lab\\\\)?(?:build|interactive-entry)\\.ps1(?:\"|\\s|$)'}); "
                "foreach($worker in $workers){taskkill.exe /PID $worker.ProcessId /T /F | Out-Null}; "
                "Get-Process sniptale-native-agent,chrome,msedge,cargo,rustc -ErrorAction SilentlyContinue | Stop-Process -Force; "
                "Write-Output 'development-recovery-complete'"
            ),
            timeout=90,
        )
        if self.boot_id() != state["boot_id"]:
            raise IntegrityError("guest rebooted during recovery")
        state["health"] = "ready"
        write_json(state_path(self.paths), state)
        write_json(
            self.paths.lab / "sessions" / state["id"] / "recovery.json",
            {
                "schema_version": 1,
                "session_id": state["id"],
                "boot_id": state["boot_id"],
                "epoch": int(time.time()),
                "release_admissible": False,
            },
        )
        return {"recovered": True, "session_id": state["id"], "boot_id": state["boot_id"], "release_admissible": False}

    def iterate(self, operation, profile="dev", cases=None, extension_artifact=None, extension_source=None):
        started = time.monotonic()
        timings = {}
        with phase(timings, "preflight"):
            self.validate_toolchain()
            state = self.require()
            require_license(self.ssh, min(3600, max(1, state["expires_epoch"] - int(time.time()))))
        if state["iterations"] >= MAX_ITERATIONS:
            raise UsageError("session iteration limit reached; close and reopen")
        current_lock = sha256_file(self.paths.root / "Cargo.lock")
        refresh_vendor = current_lock != state["vendor_lock"]
        if refresh_vendor and operation != "build":
            raise UsageError("dependency lock changed; build before testing")
        iteration, directory = create_run_dir(self.paths.lab / "sessions" / state["id"] / "iterations")
        runner = LaneRunner(self.paths, self.config)
        runner.work_session = True
        receipt: dict[str, Any] = dict(
            schema_version=1,
            mode="development-session",
            release_admissible=False,
            session_id=state["id"],
            iteration=iteration,
            boot_id=state["boot_id"],
            golden_sha256=state["golden_sha256"],
            operation=operation,
            timings_ms=timings,
            result="fail",
        )
        receipt["snapshot"] = read_json(self.paths.golden / LANE / "identity.json")
        if (extension_artifact is None) != (extension_source is None):
            raise UsageError("extension artifact and source repository must be supplied together")
        if cases is not None and "real-extension" in cases and extension_artifact is None:
            raise UsageError("real extension case requires its artifact and source repository")
        # Invalidate before mutating the guest. Interruptions cannot leave a reusable, unknown state.
        state["iterations"] += 1
        write_json(state_path(self.paths), state)
        try:
            if operation == "build":
                with phase(timings, "source"):
                    source = create_source_bundle(self.paths.root, directory / "source.tar.gz")
                    receipt["source"] = source.as_dict()
                output = directory / "artifacts"
                output.mkdir()
                if refresh_vendor:
                    with phase(timings, "dependencies"):
                        vendor = directory / "vendor"
                        try:
                            run(
                                ["cargo", "vendor", "--locked", "--offline", "--versioned-dirs", str(vendor)],
                                cwd=self.paths.root,
                                timeout=300,
                                log=directory / "logs/vendor.log",
                            )
                            runner.vendor_bundle = directory / "vendor.tar.gz"
                            with tarfile.open(runner.vendor_bundle, "w:gz", compresslevel=1) as bundle:
                                bundle.add(vendor, arcname="vendor", filter=vendor_metadata)
                            if runner.vendor_bundle.stat().st_size > 512 * 1024 * 1024:
                                raise ResourceError("vendored dependencies exceed transfer budget")
                            if sha256_file(self.paths.root / "Cargo.lock") != current_lock:
                                raise IntegrityError("dependency lock changed while preparing offline sources")
                        finally:
                            if vendor.exists():
                                shutil.rmtree(self.paths.scoped(vendor))
                    state["vendor_lock"] = "0" * 64
                state["health"] = "failed"
                write_json(state_path(self.paths), state)
                with phase(timings, "build"):
                    runner._build_windows(LANE, profile, source, output)
                # A completed guest build proves the transferred dependency set. Retain that
                # input cache even if a later source-stability check rejects the artifacts.
                state["vendor_lock"] = current_lock
                write_json(state_path(self.paths), state)
                after = create_source_bundle(self.paths.root, directory / "source-check.tar.gz")
                after.archive.unlink()
                if after.as_dict() != source.as_dict():
                    raise IntegrityError("desktop sources changed during build")
                artifact = next(output.rglob(f"*0.1.0-{profile}.msi"))
                manifest = read_json(artifact.parent / "build-manifest.json")
                if (
                    not isinstance(manifest, dict)
                    or not isinstance(manifest.get("environment"), dict)
                    or not isinstance(manifest.get("timings_ms"), dict)
                ):
                    raise IntegrityError("guest build environment and phase timings missing")
                receipt["guest_build"] = {"environment": manifest["environment"], "timings_ms": manifest["timings_ms"]}
                state["vendor_lock"] = current_lock
                if runner.vendor_bundle is not None:
                    runner.vendor_bundle.unlink()
                state["last_artifact"] = artifact.relative_to(self.paths.lab).as_posix()
                shutil.move(output / "build.log", directory / "logs/windows-build.log")
                receipt["artifacts"] = [
                    {"name": p.name, "sha256": sha256_file(p), "bytes": p.stat().st_size}
                    for p in sorted(output.rglob("*"))
                    if p.is_file()
                ]
                source.archive.unlink()
            else:
                if state["last_artifact"] is None:
                    raise UsageError("build the current sources before testing")
                artifact = self.paths.scoped(self.paths.lab / state["last_artifact"])
                build_dir = next(
                    (
                        parent
                        for parent in artifact.parents
                        if parent.is_relative_to(self.paths.lab / "sessions" / state["id"])
                        and (parent / "development-receipt.json").is_file()
                    ),
                    None,
                )
                build_receipt = read_json(build_dir / "development-receipt.json") if build_dir else None
                if (
                    build_dir is None
                    or build_receipt is None
                    or build_receipt.get("mode") != "development-session"
                    or build_receipt.get("result") != "pass"
                    or build_receipt.get("operation") != "build"
                ):
                    raise IntegrityError("successful development build evidence missing")
                with phase(timings, "source-verification"):
                    current = create_source_bundle(self.paths.root, directory / "source-check.tar.gz")
                    current.archive.unlink()
                    if current.as_dict() != build_receipt.get("source"):
                        raise IntegrityError("desktop sources changed since build; rebuild before testing")
                    receipt["source"] = current.as_dict()
                    entries = build_receipt.get("artifacts")
                    actual = [
                        {"name": p.name, "sha256": sha256_file(p), "bytes": p.stat().st_size}
                        for p in sorted((build_dir / "artifacts").rglob("*"))
                        if p.is_file()
                    ]
                    if entries != actual:
                        raise IntegrityError("development build artifact inventory changed")
                receipt["artifact_sha256"] = sha256_file(artifact)
                if extension_artifact is not None:
                    if extension_source is None:
                        raise UsageError("extension source repository is required")
                    from extension_artifact import package_extension

                    with phase(timings, "extension-bundle"):
                        runner.real_extension, receipt["extension"] = package_extension(
                            self.paths, extension_artifact, extension_source, directory
                        )
                with phase(timings, "test"):
                    state["health"] = "failed"
                    write_json(state_path(self.paths), state)
                    result = runner.test(LANE, artifact, directory, cases)
                write_junit(directory / "junit.xml", [result])
                receipt["cases"] = [
                    {"name": c.name, "status": c.status, "duration_ms": c.duration_ms} for c in result.cases
                ]
                if not result.passed:
                    raise LabError("development guest tests failed; inspect iteration evidence")
                after = create_source_bundle(self.paths.root, directory / "source-check.tar.gz")
                after.archive.unlink()
                if after.as_dict() != receipt["source"]:
                    raise IntegrityError("desktop sources changed during testing")
            if self.boot_id() != state["boot_id"]:
                raise IntegrityError("guest rebooted during iteration")
            if time.time() >= state["expires_epoch"]:
                raise LabError("development session expired during iteration")
            receipt["result"] = "pass"
            state["health"] = "ready"
            return {
                "iteration": iteration,
                "session_id": state["id"],
                "result": "pass",
                "timings_ms": timings,
                "release_admissible": False,
            }
        finally:
            timings["total"] = round((time.monotonic() - started) * 1000)
            receipt["resources"] = {
                "temporary_bytes": directory_allocated_bytes(self.paths.lab),
                "maximum_temporary_bytes": self.config.limits.max_temporary_gib * 1024**3,
            }
            emit(
                "session-resources",
                status="measured",
                temporaryBytes=receipt["resources"]["temporary_bytes"],
                maximumTemporaryBytes=receipt["resources"]["maximum_temporary_bytes"],
            )
            from session_evidence import inventory

            LaneRunner._sanitize_guest_text_tree(directory / "logs")
            receipt["evidence"] = inventory(directory)
            write_json(directory / "development-receipt.json", receipt)
            write_json(state_path(self.paths), state)


def gc_sessions(paths, config):
    """Remove only inactive, bounded session directories through the controller GC."""
    root = paths.scoped(paths.lab / "sessions")
    if not root.exists():
        return 0
    from session_evidence import IDENTITY

    candidates = sorted(
        (p for p in root.iterdir() if IDENTITY.fullmatch(p.name)), key=lambda p: p.stat().st_mtime, reverse=True
    )
    removed = 0
    for index, path in enumerate(candidates):
        if path.is_symlink():
            raise IntegrityError("session directory is redirected")
        if (
            index >= config.limits.max_runs
            or time.time() - path.stat().st_mtime > config.limits.max_run_age_days * 86400
        ):
            shutil.rmtree(paths.scoped(path))
            removed += 1
    return removed
