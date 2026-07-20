"""QEMU overlay lifecycle with local-only endpoints and forced teardown."""

from __future__ import annotations

import json
import os
import re
import shutil
import socket
import stat
import subprocess
import time
from dataclasses import dataclass
from pathlib import Path

from config import Lane, LabConfig
from errors import IntegrityError, LabError
from paths import LabPaths
from state import process_matches, read_json, write_json
from system import run, sha256_file, terminate_process, wait_for_resources


@dataclass(frozen=True)
class RunningVm:
    lane: str
    pid: int
    overlay: Path
    qmp_socket: Path
    started_epoch: int
    golden_sha256: str
    guest_network: bool
    windows_audio_capture: bool = False


class VmManager:
    def __init__(self, paths: LabPaths, config: LabConfig) -> None:
        self.paths = paths
        self.config = config

    def state_path(self, lane: str) -> Path:
        return self.paths.runtime / lane / "state.json"

    def current(self, lane: str, *, clean_stale: bool = True) -> RunningVm | None:
        runtime = self.paths.runtime / lane
        if runtime.is_symlink() or runtime.resolve(strict=False) != runtime:
            raise IntegrityError(f"runtime directory is redirected for {lane}")
        state = read_json(runtime / "state.json")
        if state is None:
            return None
        required = {
            "lane", "pid", "overlay", "qmp_socket", "started_epoch", "golden_sha256",
            "guest_network", "windows_audio_capture",
        }
        if set(state) != required or state["lane"] != lane:
            raise IntegrityError(f"corrupted VM state for {lane}")
        if (
            not isinstance(state["pid"], int) or isinstance(state["pid"], bool) or state["pid"] <= 1
            or not isinstance(state["started_epoch"], int) or isinstance(state["started_epoch"], bool)
            or not isinstance(state["guest_network"], bool)
            or not isinstance(state["windows_audio_capture"], bool)
            or not isinstance(state["overlay"], str) or Path(state["overlay"]) != runtime / "overlay.qcow2"
            or not isinstance(state["qmp_socket"], str) or Path(state["qmp_socket"]) != runtime / "qmp.sock"
            or not isinstance(state["golden_sha256"], str) or not re.fullmatch(r"[0-9a-f]{64}", state["golden_sha256"])
        ):
            raise IntegrityError(f"corrupted VM state types or paths for {lane}")
        vm = RunningVm(
            lane=lane, pid=int(state["pid"]), overlay=Path(state["overlay"]),
            qmp_socket=Path(state["qmp_socket"]), started_epoch=int(state["started_epoch"]),
            golden_sha256=str(state["golden_sha256"]), guest_network=bool(state["guest_network"]),
            windows_audio_capture=bool(state["windows_audio_capture"]),
        )
        marker = f"sniptale-lab-{lane}"
        if process_matches(vm.pid, marker, str(self.paths.lab)):
            return vm
        if clean_stale:
            self._remove_runtime(lane)
            return None
        raise IntegrityError(f"stale VM state for {lane}")

    def _assert_only_one(self, lane: str) -> None:
        active = [name for name in self.config.lanes if name != lane and self.current(name) is not None]
        for name in self.config.lanes:
            if name != lane and self.orphan_pids(name):
                active.append(name)
        if active:
            raise LabError(f"only one heavy VM may run; active lane: {active[0]}")

    def orphan_pids(self, lane: str) -> list[int]:
        marker = f"sniptale-lab-{lane}"
        lab_marker = str(self.paths.lab)
        pids = []
        for entry in Path("/proc").iterdir():
            if not entry.name.isdigit():
                continue
            try:
                command = (entry / "cmdline").read_bytes().replace(b"\0", b" ").decode("utf-8", "replace")
            except OSError:
                continue
            if "qemu-system" in command and marker in command and lab_marker in command:
                pids.append(int(entry.name))
        return pids

    def _qemu(self) -> str:
        candidate = os.environ.get("SNIPTALE_E2E_QEMU") or shutil.which("qemu-system-x86_64")
        if not candidate:
            bundled = self.paths.cache / "host-tools/bin/qemu-system-x86_64"
            if bundled.exists():
                return str(bundled)
            raise LabError("qemu-system-x86_64 is not installed; run tooling/desktop/e2e-lab bootstrap")
        return candidate

    def start(
        self,
        lane_name: str,
        *,
        allow_guest_network: bool = False,
        windows_audio_capture: bool = False,
        host_reserve_mib: int = 2048,
    ) -> RunningVm:
        existing = self.current(lane_name)
        if existing:
            if (
                existing.guest_network != allow_guest_network
                or existing.windows_audio_capture != windows_audio_capture
            ):
                raise LabError(f"{lane_name} is already running with a different VM policy")
            return existing
        if self.orphan_pids(lane_name):
            raise LabError(f"unmanaged QEMU process exists for {lane_name}; run reset {lane_name}")
        self._assert_only_one(lane_name)
        lane = self.config.lanes[lane_name]
        if host_reserve_mib < 1024 or host_reserve_mib > 4096:
            raise IntegrityError("VM host memory reserve must be 1024..4096 MiB")
        wait_for_resources(
            self.paths.lab, min_free_gib=self.config.limits.min_free_gib,
            min_memory_mib=max(
                self.config.limits.min_available_memory_mib,
                lane.memory_mib + host_reserve_mib,
            ),
            timeout=self.config.timeouts.shutdown,
        )
        golden_dir = self.paths.golden / lane_name
        if golden_dir.is_symlink() or golden_dir.resolve(strict=False) != golden_dir:
            raise IntegrityError(f"golden image directory is redirected for {lane_name}")
        golden = golden_dir / "disk.qcow2"
        identity = golden_dir / "identity.json"
        if golden.is_symlink() or identity.is_symlink() or not golden.is_file() or not identity.is_file():
            raise LabError(f"golden image is missing for {lane_name}; run provision {lane_name}")
        identity_value = read_json(identity)
        base_fields = {
            "schema_version", "lane", "os", "display_server", "provision_acceleration",
            "source_sha256", "disk_sha256", "created_epoch",
        }
        expected_fields = base_fields | ({"derived_from_disk_sha256"} if lane_name == "linux-x11" else set())
        if identity_value is not None and "storage_format" in identity_value:
            expected_fields.add("storage_format")
        if lane.os == "windows":
            expected_fields |= {
                "firmware_code_sha256", "firmware_vars_sha256", "guest_edition",
                "guest_display_version", "guest_build", "source_release", "source_provenance",
            }
        expected_os = "windows-11-enterprise-evaluation" if lane.os == "windows" else "ubuntu-24.04"
        def valid_hash(value: object) -> bool:
            return isinstance(value, str) and re.fullmatch(r"[0-9a-f]{64}", value) is not None
        if (
            identity_value is None
            or set(identity_value) != expected_fields
            or identity_value.get("schema_version") != 1
            or identity_value.get("lane") != lane_name
            or identity_value.get("os") != expected_os
            or identity_value.get("display_server") != lane.display_server
            or identity_value.get("provision_acceleration") not in {"kvm", "tcg"}
            or identity_value.get("storage_format", "qcow2-legacy-v1") not in {
                "qcow2-legacy-v1", "qcow2-zlib-v1",
            }
            or not valid_hash(identity_value.get("source_sha256"))
            or not valid_hash(identity_value.get("disk_sha256"))
            or not isinstance(identity_value.get("created_epoch"), int)
            or isinstance(identity_value.get("created_epoch"), bool)
            or identity_value["created_epoch"] <= 0
            or identity_value["disk_sha256"] != sha256_file(golden)
            or any(path.stat().st_mode & (stat.S_IWUSR | stat.S_IWGRP | stat.S_IWOTH) for path in (golden, identity))
        ):
            raise IntegrityError(f"golden image identity mismatch for {lane_name}")
        if lane.os == "windows":
            if (
                identity_value.get("guest_edition") != "EnterpriseEval"
                or identity_value.get("guest_display_version") != "25H2"
                or identity_value.get("source_release") != "Windows 11 Enterprise Evaluation 25H2 en-us x64"
                or identity_value.get("source_provenance") != "microsoft-evaluation-center-published-sha256"
                or not isinstance(identity_value.get("guest_build"), int)
                or identity_value["guest_build"] < 26200
            ):
                raise IntegrityError(f"golden guest OS identity mismatch for {lane_name}")
            firmware_vars = self.paths.golden / lane_name / "OVMF_VARS.fd"
            firmware_code = self.paths.golden / lane_name / "OVMF_CODE.fd"
            if (
                firmware_vars.is_symlink() or not firmware_vars.is_file()
                or not valid_hash(identity_value.get("firmware_vars_sha256"))
                or identity_value.get("firmware_vars_sha256") != sha256_file(firmware_vars)
                or firmware_vars.stat().st_mode & (stat.S_IWUSR | stat.S_IWGRP | stat.S_IWOTH)
            ):
                raise IntegrityError(f"golden firmware identity mismatch for {lane_name}")
            if (
                firmware_code.is_symlink() or not firmware_code.is_file()
                or not valid_hash(identity_value.get("firmware_code_sha256"))
                or identity_value.get("firmware_code_sha256") != sha256_file(firmware_code)
                or firmware_code.stat().st_mode & (stat.S_IWUSR | stat.S_IWGRP | stat.S_IWOTH)
            ):
                raise IntegrityError(f"golden firmware code identity mismatch for {lane_name}")
        qemu_img = shutil.which("qemu-img") or str(self.paths.cache / "host-tools/bin/qemu-img")
        image_info = run([qemu_img, "info", "--output=json", str(golden)], timeout=60)
        try:
            image_metadata = json.loads(image_info.stdout)
        except json.JSONDecodeError as exc:
            raise IntegrityError(f"golden image metadata is invalid for {lane_name}") from exc
        if not isinstance(image_metadata, dict) or image_metadata.get("backing-filename"):
            raise IntegrityError(f"golden image must be a self-contained flat qcow2 for {lane_name}")
        runtime = self._fresh_runtime(lane_name)
        overlay = runtime / "overlay.qcow2"
        qmp = runtime / "qmp.sock"
        run([qemu_img, "create", "-q", "-f", "qcow2", "-F", "qcow2", "-b", str(golden), str(overlay)], timeout=60)
        command = self._command(
            lane, overlay, qmp, runtime,
            allow_guest_network=allow_guest_network,
            windows_audio_capture=windows_audio_capture,
        )
        log_path = runtime / "qemu.log"
        log_stream = log_path.open("xb", buffering=0)
        environment = os.environ.copy()
        if lane.os == "windows" and windows_audio_capture and not allow_guest_network:
            environment["ALSA_CONFIG_PATH"] = str(self._prepare_windows_microphone(runtime))
        process = subprocess.Popen(
            command, stdin=subprocess.DEVNULL, stdout=log_stream, stderr=subprocess.STDOUT,
            start_new_session=True, env=environment,
        )
        log_stream.close()
        time.sleep(1)
        if process.poll() is not None:
            tail = log_path.read_text(encoding="utf-8", errors="replace")[-4096:]
            self._remove_runtime(lane_name)
            raise LabError(f"QEMU failed to start for {lane_name}: {tail}")
        state = RunningVm(
            lane_name, process.pid, overlay, qmp, int(time.time()),
            identity_value["disk_sha256"], allow_guest_network, windows_audio_capture,
        )
        try:
            write_json(self.state_path(lane_name), {
                "lane": state.lane, "pid": state.pid, "overlay": str(state.overlay),
                "qmp_socket": str(state.qmp_socket), "started_epoch": state.started_epoch,
                "golden_sha256": state.golden_sha256, "guest_network": state.guest_network,
                "windows_audio_capture": state.windows_audio_capture,
            })
        except Exception:
            terminate_process(process.pid, timeout=5)
            self._remove_runtime(lane_name)
            raise
        return state

    def _fresh_runtime(self, lane_name: str) -> Path:
        runtime = self.paths.runtime / lane_name
        if runtime.is_symlink():
            raise IntegrityError(f"runtime directory is redirected for {lane_name}")
        if runtime.exists():
            shutil.rmtree(self.paths.scoped(runtime))
        runtime.mkdir(parents=True, mode=0o700)
        return runtime

    def _command(
        self,
        lane: Lane,
        overlay: Path,
        qmp: Path,
        runtime: Path,
        *,
        allow_guest_network: bool = False,
        windows_audio_capture: bool = False,
    ) -> list[str]:
        acceleration = "kvm" if os.access("/dev/kvm", os.R_OK | os.W_OK) else "tcg"
        command = [
            self._qemu(), "-name", f"sniptale-lab-{lane.name},process=sniptale-{lane.name}",
            "-machine", "q35", "-accel", acceleration if acceleration == "kvm" else "tcg,thread=multi", "-cpu", "host" if acceleration == "kvm" else "max",
            "-smp", str(lane.vcpus), "-m", str(lane.memory_mib), "-no-reboot", "-monitor", "none",
            "-drive", f"if=none,id=system,file={overlay},format=qcow2,cache=none,discard=unmap",
            "-device", "virtio-blk-pci,drive=system,addr=0x2",
            "-netdev", f"user,id=net0,restrict={'off' if allow_guest_network else 'on'},hostfwd=tcp:127.0.0.1:{lane.ssh_port}-:22",
            "-device", "virtio-net-pci,netdev=net0,mac=52:54:00:12:34:56,addr=0x1", "-device", "virtio-vga,addr=0x3",
            "-device", "qemu-xhci", "-device", "usb-kbd", "-device", "usb-tablet",
            "-display", "none", "-vnc", f"127.0.0.1:{lane.vnc_display}",
            "-qmp", f"unix:{qmp},server=on,wait=off", "-serial", f"file:{runtime / 'serial.log'}",
        ]
        if lane.os == "windows":
            command[command.index("virtio-blk-pci,drive=system,addr=0x2")] = "nvme,drive=system,serial=sniptalelab,addr=0x2"
            command[command.index("virtio-net-pci,netdev=net0,mac=52:54:00:12:34:56,addr=0x1")] = "e1000e,netdev=net0,mac=52:54:00:12:34:56,addr=0x1"
            command[command.index("virtio-vga,addr=0x3")] = "VGA,addr=0x3"
            if allow_guest_network:
                command.extend([
                    "-audiodev", "driver=none,id=audio0",
                    "-device", "ich9-intel-hda",
                    "-device", "hda-duplex,audiodev=audio0",
                ])
            elif windows_audio_capture:
                command.extend([
                    "-audiodev",
                    "driver=alsa,id=audio0,in.dev=sniptale_e2e_mic,out.dev=sniptale_e2e_speaker",
                    "-device", "ich9-intel-hda",
                    "-device", "hda-duplex,audiodev=audio0",
                ])
            else:
                # System-loopback acceptance needs a timer-paced render sink. The duplex
                # codec is intentional: Windows loopback capture did not receive packets
                # from QEMU's output-only codec, while the WAV backend keeps render paced.
                command.extend([
                    "-audiodev", f"driver=wav,id=audio0,path={runtime / 'system-audio-output.wav'}",
                    "-device", "ich9-intel-hda",
                    "-device", "hda-duplex,audiodev=audio0",
                ])
            self._append_windows_firmware(
                command, runtime,
                persistent_code=self.paths.golden / f"{lane.name}/OVMF_CODE.fd",
                persistent_vars=self.paths.golden / f"{lane.name}/OVMF_VARS.fd",
            )
        return command

    @staticmethod
    def _prepare_windows_microphone(runtime: Path) -> Path:
        config = runtime / "alsa-sniptale.conf"
        config.write_text(
            "</usr/share/alsa/alsa.conf>\n"
            "pcm.sniptale_e2e_mic {\n"
            "  type file\n"
            "  slave.pcm null\n"
            "  file \"/dev/null\"\n"
            # QEMU can open and consume its capture backend long before the microphone case. An
            # on-disk tone therefore has a hidden time limit and previously reached EOF late in a
            # full run. /dev/urandom is an infinite synthetic, non-user source and avoids a large
            # short-lived raw fixture while still proving that the captured PCM is non-silent.
            "  infile \"/dev/urandom\"\n"
            "  format raw\n"
            "}\n"
            "pcm.sniptale_e2e_speaker {\n"
            "  type file\n"
            "  slave.pcm null\n"
            f"  file \"{runtime / 'system-audio-output-s16le.raw'}\"\n"
            "  format raw\n"
            "}\n",
            encoding="utf-8",
        )
        config.chmod(0o600)
        return config

    def _append_windows_firmware(
        self,
        command: list[str],
        runtime: Path,
        *,
        persistent_code: Path | None = None,
        persistent_vars: Path | None = None,
    ) -> None:
        bundled = self.paths.cache / "host-tools/usr/share/OVMF"
        bundled_legacy = self.paths.cache / "host-tools/usr/share/ovmf"
        code_candidates = tuple(path for path in (persistent_code,) if path is not None) + (
            Path("/usr/share/OVMF/OVMF_CODE_4M.ms.fd"), Path("/usr/share/OVMF/OVMF_CODE_4M.secboot.fd"),
            Path("/usr/share/OVMF/OVMF_CODE_4M.fd"), Path("/usr/share/OVMF/OVMF_CODE.fd"),
            bundled / "OVMF_CODE_4M.ms.fd", bundled / "OVMF_CODE_4M.secboot.fd",
            bundled / "OVMF_CODE_4M.fd", bundled / "OVMF_CODE.fd", bundled_legacy / "OVMF.fd",
        )
        vars_candidates = tuple(path for path in (persistent_vars,) if path is not None) + (
            Path("/usr/share/OVMF/OVMF_VARS_4M.ms.fd"), Path("/usr/share/OVMF/OVMF_VARS.fd"),
            bundled / "OVMF_VARS_4M.ms.fd", bundled / "OVMF_VARS.fd",
        )
        code = next((path for path in code_candidates if path.exists()), None)
        template = next((path for path in vars_candidates if path.exists()), None)
        if code is None or template is None:
            raise LabError("Windows VM requires OVMF firmware; run bootstrap")
        code_path = runtime / "OVMF_CODE.fd"
        if code.resolve() != code_path.resolve():
            shutil.copy2(code, code_path)
        code = code_path
        vars_path = runtime / "OVMF_VARS.fd"
        if not vars_path.exists():
            shutil.copy2(template, vars_path)
        vars_path.chmod(0o600)
        command.extend(["-drive", f"if=pflash,format=raw,readonly=on,file={code}", "-drive", f"if=pflash,format=raw,file={vars_path}"])

    def qmp(self, vm: RunningVm, execute: str, arguments: dict[str, object] | None = None) -> None:
        deadline = time.monotonic() + 10
        while not vm.qmp_socket.exists() and time.monotonic() < deadline:
            time.sleep(0.1)
        if not vm.qmp_socket.exists():
            raise LabError("QMP socket did not become available")
        try:
            with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as client:
                client.settimeout(10)
                client.connect(str(vm.qmp_socket))
                if not client.recv(65536):
                    raise LabError("QMP closed before its greeting")
                client.sendall(b'{"execute":"qmp_capabilities"}\r\n')
                if not client.recv(65536):
                    raise LabError("QMP closed during capability negotiation")
                request: dict[str, object] = {"execute": execute}
                if arguments is not None:
                    request["arguments"] = arguments
                client.sendall(json.dumps(request).encode() + b"\r\n")
                response = client.recv(65536)
                if not response:
                    raise LabError(f"QMP closed while executing {execute}")
                try:
                    messages = [json.loads(line) for line in response.splitlines() if line.strip()]
                except json.JSONDecodeError as exc:
                    raise LabError(f"QMP returned an invalid response for {execute}") from exc
                error = next((message["error"] for message in messages if isinstance(message, dict) and "error" in message), None)
                if error is not None:
                    raise LabError(f"QMP rejected {execute}")
                if not any(isinstance(message, dict) and "return" in message for message in messages):
                    raise LabError(f"QMP did not acknowledge {execute}")
        except OSError as exc:
            raise LabError("QMP transport failed") from exc

    def stop(self, lane_name: str) -> bool:
        try:
            vm = self.current(lane_name)
        except IntegrityError:
            forced = False
            for pid in self.orphan_pids(lane_name):
                forced = terminate_process(pid, timeout=5) or forced
            self._remove_runtime(lane_name)
            return forced
        if vm is None:
            forced = False
            for pid in self.orphan_pids(lane_name):
                forced = terminate_process(pid, timeout=5) or forced
            return forced
        forced = False
        try:
            self.qmp(vm, "system_powerdown")
        except LabError:
            pass
        deadline = time.monotonic() + self.config.timeouts.shutdown
        while process_matches(vm.pid, f"sniptale-lab-{lane_name}", str(self.paths.lab)) and time.monotonic() < deadline:
            time.sleep(0.5)
        if process_matches(vm.pid, f"sniptale-lab-{lane_name}", str(self.paths.lab)):
            forced = terminate_process(vm.pid, timeout=5)
        self._remove_runtime(lane_name)
        return forced

    def reset(self, lane_name: str) -> bool:
        forced = self.stop(lane_name)
        self._remove_runtime(lane_name)
        return forced

    def _remove_runtime(self, lane: str) -> None:
        owned = self.paths.runtime / lane
        if owned.is_symlink():
            raise IntegrityError(f"runtime directory is redirected for {lane}")
        runtime = self.paths.scoped(owned)
        if runtime.exists():
            shutil.rmtree(runtime)
