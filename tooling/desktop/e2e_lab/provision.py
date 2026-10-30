"""Golden image provisioning from checksum-bound official media."""

from __future__ import annotations

import base64
import json
import os
import re
import shutil
import subprocess
import time
import urllib.request
import urllib.parse
from pathlib import Path

from config import Lane, LabConfig
from errors import IntegrityError, LabError, TimeoutExpired
from golden_storage import STORAGE_FORMAT, compact_unsealed_disk
from paths import LabPaths
from ssh import GuestSsh, powershell_encoded
from state import read_json, write_json
from system import download_https, require_sha256, run, sha256_file, terminate_process, validate_resources
from vm import RunningVm, VmManager, cpu_model

UBUNTU_BASE = "https://cloud-images.ubuntu.com/noble/current"
UBUNTU_IMAGE = "noble-server-cloudimg-amd64.img"
OFFICIAL_WINDOWS_MEDIA = {
    # Microsoft Evaluation Center, Windows 11 Enterprise 25H2, English (United States), x64.
    "a61adeab895ef5a4db436e0a7011c92a2ff17bb0357f58b13bbc4062e535e7b9": {
        "edition": "EnterpriseEval",
        "display_version": "25H2",
        "minimum_build": 26200,
        "release": "Windows 11 Enterprise Evaluation 25H2 en-us x64",
    },
}


class Provisioner:
    def __init__(self, paths: LabPaths, config: LabConfig) -> None:
        self.paths = paths
        self.config = config

    def tool(self, name: str) -> str:
        installed = shutil.which(name)
        bundled = self.paths.cache / f"host-tools/bin/{name}"
        if installed:
            return installed
        if bundled.exists():
            return str(bundled)
        raise LabError(f"missing host tool {name}; run bootstrap")

    def provision(self, lane_name: str) -> dict[str, object]:
        lane = self.config.lanes[lane_name]
        validate_resources(
            self.paths.lab,
            min_free_gib=self.config.limits.min_free_gib,
            min_memory_mib=max(self.config.limits.min_available_memory_mib, lane.memory_mib + 2048),
        )
        if lane.os == "linux":
            return self._linux(lane)
        return self._windows(lane)

    @staticmethod
    def _seal(*paths: Path) -> None:
        for path in paths:
            path.chmod(0o444)

    def _ubuntu_image(self) -> tuple[Path, str]:
        request = urllib.request.Request(
            f"{UBUNTU_BASE}/SHA256SUMS", headers={"User-Agent": "SniptaleDesktopE2ELab/1"},
        )
        with urllib.request.urlopen(request, timeout=120) as response:
            final = urllib.parse.urlparse(response.geturl())
            if final.scheme != "https" or final.hostname != "cloud-images.ubuntu.com":
                raise IntegrityError("Ubuntu checksum manifest redirected away from its official HTTPS host")
            manifest_bytes = response.read(1024 * 1024 + 1)
            if len(manifest_bytes) > 1024 * 1024:
                raise IntegrityError("Ubuntu checksum manifest exceeds the 1 MiB limit")
            manifest = manifest_bytes.decode("utf-8")
        expected = ""
        for line in manifest.splitlines():
            fields = line.split()
            if len(fields) == 2 and fields[1].lstrip("*") == UBUNTU_IMAGE:
                expected = fields[0]
                break
        if len(expected) != 64:
            raise IntegrityError("official Ubuntu SHA256SUMS does not contain the expected image")
        destination = self.paths.images / f"ubuntu-noble-{expected}.img"
        if destination.exists():
            require_sha256(destination, expected)
        else:
            download_https(f"{UBUNTU_BASE}/{UBUNTU_IMAGE}", destination, expected, timeout=3600)
        return destination, expected

    def _seed_iso(self, lane: Lane, destination: Path) -> None:
        public_key = (self.paths.cache / "ssh/lab_ed25519.pub").read_text(encoding="utf-8").strip()
        provision_script = (self.paths.root / "tooling/desktop/e2e_lab/guest/linux-provision.sh").read_bytes()
        encoded = base64.b64encode(provision_script).decode("ascii")
        session_exec = (self.paths.root / "tooling/desktop/e2e_lab/guest/linux-session-exec.sh").read_bytes()
        session_exec_encoded = base64.b64encode(session_exec).decode("ascii")
        seed = destination.parent / "seed"
        if seed.exists():
            shutil.rmtree(seed)
        seed.mkdir(mode=0o700)
        user_data = (
            "#cloud-config\n"
            "users:\n"
            "  - name: lab\n"
            "    gecos: Sniptale Lab\n"
            "    groups: [adm, audio, cdrom, dialout, plugdev, sudo, video]\n"
            "    sudo: ALL=(ALL) NOPASSWD:ALL\n"
            "    shell: /bin/bash\n"
            "    lock_passwd: true\n"
            "    ssh_authorized_keys:\n"
            f"      - {public_key}\n"
            "ssh_pwauth: false\n"
            "disable_root: true\n"
            "write_files:\n"
            "  - path: /usr/local/sbin/sniptale-lab-provision\n"
            "    owner: root:root\n"
            "    permissions: '0755'\n"
            "    encoding: b64\n"
            f"    content: {encoded}\n"
            "  - path: /usr/local/bin/sniptale-session-exec\n"
            "    owner: root:root\n"
            "    permissions: '0755'\n"
            "    encoding: b64\n"
            f"    content: {session_exec_encoded}\n"
            "runcmd:\n"
            f"  - [bash, /usr/local/sbin/sniptale-lab-provision, {lane.name}]\n"
            "final_message: Sniptale lab cloud-init complete\n"
        )
        (seed / "user-data").write_text(user_data, encoding="utf-8")
        (seed / "meta-data").write_text(
            f"instance-id: sniptale-{lane.name}-v1\nlocal-hostname: sniptale-{lane.display_server}\n",
            encoding="utf-8",
        )
        run(
            [self.tool("genisoimage"), "-quiet", "-output", str(destination), "-volid", "cidata", "-joliet", "-rock", str(seed / "user-data"), str(seed / "meta-data")],
            timeout=120,
        )

    def _linux(self, lane: Lane) -> dict[str, object]:
        if lane.name == "linux-x11":
            derived = self._linux_from_wayland(lane)
            if derived is not None:
                return derived
        image, image_hash = self._ubuntu_image()
        target_dir = self.paths.golden / lane.name
        temporary_dir = self.paths.golden / f".{lane.name}.provisioning"
        if temporary_dir.exists():
            shutil.rmtree(temporary_dir)
        temporary_dir.mkdir(parents=True, mode=0o700)
        disk = temporary_dir / "disk.qcow2"
        seed = temporary_dir / "seed.iso"
        run(
            [self.tool("qemu-img"), "create", "-q", "-f", "qcow2", "-F", "qcow2", "-b", str(image), str(disk), f"{lane.disk_gib}G"],
            timeout=120,
        )
        self._seed_iso(lane, seed)
        process, log, acceleration = self._launch_linux(lane, disk, seed, temporary_dir)
        time.sleep(1)
        if process.poll() is not None:
            raise LabError(f"Linux provisioning QEMU failed to start: {log.read_text(encoding='utf-8', errors='replace')[-4096:]}")
        ssh = GuestSsh(self.paths, lane.ssh_port, "lab")
        ssh.forget_host_key()
        try:
            ssh.wait(self.config.timeouts.boot)
            deadline = time.monotonic() + self.config.timeouts.provision
            sentinel_seen = False
            while time.monotonic() < deadline:
                if process.poll() is not None:
                    break
                result = subprocess.run(
                    ["ssh", *ssh.options(), "lab@127.0.0.1", "--", "sudo", "test", "-f", "/var/lib/sniptale-e2e/provisioned"],
                    stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False,
                )
                if result.returncode == 0:
                    sentinel_seen = True
                    break
                time.sleep(10)
            if not sentinel_seen and process.poll() is None:
                raise TimeoutExpired(f"Linux provisioning timed out for {lane.name}")
            try:
                process.wait(timeout=self.config.timeouts.shutdown + 120)
            except subprocess.TimeoutExpired as exc:
                terminate_process(process.pid, timeout=5)
                raise TimeoutExpired(f"Linux golden did not power off for {lane.name}") from exc
            if process.returncode != 0:
                tail = log.read_text(encoding="utf-8", errors="replace")[-4096:]
                raise LabError(f"Linux provisioning QEMU failed for {lane.name}: {tail}")
        except BaseException:
            if process.poll() is None:
                terminate_process(process.pid, timeout=5)
            raise
        compact_unsealed_disk(self.paths, disk)
        identity = {
            "schema_version": 1,
            "lane": lane.name,
            "os": "ubuntu-24.04",
            "display_server": lane.display_server,
            "provision_acceleration": acceleration,
            "source_sha256": image_hash,
            "disk_sha256": sha256_file(disk),
            "storage_format": STORAGE_FORMAT,
            "created_epoch": int(time.time()),
        }
        for disposable in (seed, temporary_dir / "seed", log, temporary_dir / "serial.log"):
            if disposable.is_dir():
                shutil.rmtree(disposable)
            else:
                disposable.unlink(missing_ok=True)
        write_json(temporary_dir / "identity.json", identity)
        self._seal(disk, temporary_dir / "identity.json")
        if target_dir.exists():
            shutil.rmtree(target_dir)
        os.replace(temporary_dir, target_dir)
        return identity

    def _linux_from_wayland(self, lane: Lane) -> dict[str, object] | None:
        source_dir = self.paths.golden / "linux-wayland"
        source_disk = source_dir / "disk.qcow2"
        source_identity_path = source_dir / "identity.json"
        if not source_disk.is_file() or not source_identity_path.is_file():
            return None
        identity = read_json(source_identity_path)
        if identity.get("lane") != "linux-wayland" or identity.get("disk_sha256") != sha256_file(source_disk):
            raise IntegrityError("Wayland golden identity is invalid; refusing X11 derivation")
        target_dir = self.paths.golden / lane.name
        temporary_dir = self.paths.golden / f".{lane.name}.provisioning"
        if temporary_dir.exists():
            shutil.rmtree(temporary_dir)
        temporary_dir.mkdir(parents=True, mode=0o700)
        disk = temporary_dir / "disk.qcow2"
        run([self.tool("qemu-img"), "convert", "-q", "-O", "qcow2", str(source_disk), str(disk)], timeout=1800)
        process, log, acceleration = self._launch_linux(lane, disk, None, temporary_dir)
        time.sleep(1)
        if process.poll() is not None:
            raise LabError(f"X11 derivation QEMU failed to start: {log.read_text(encoding='utf-8', errors='replace')[-4096:]}")
        ssh = GuestSsh(self.paths, lane.ssh_port, "lab")
        ssh.forget_host_key()
        try:
            ssh.wait(self.config.timeouts.boot)
            script = self.paths.root / "tooling/desktop/e2e_lab/guest/linux-set-x11.sh"
            ssh.upload(script, "linux-set-x11.sh", timeout=120)
            ssh.command(["sudo", "bash", "linux-set-x11.sh"], timeout=120)
            try:
                process.wait(timeout=self.config.timeouts.shutdown + 120)
            except subprocess.TimeoutExpired as exc:
                terminate_process(process.pid, timeout=5)
                raise TimeoutExpired("derived X11 golden did not power off") from exc
            if process.returncode != 0:
                tail = log.read_text(encoding="utf-8", errors="replace")[-4096:]
                raise LabError(f"X11 derivation QEMU failed: {tail}")
        except BaseException:
            if process.poll() is None:
                terminate_process(process.pid, timeout=5)
            raise
        compact_unsealed_disk(self.paths, disk)
        derived_identity = {
            "schema_version": 1,
            "lane": lane.name,
            "os": "ubuntu-24.04",
            "display_server": "x11",
            "provision_acceleration": acceleration,
            "source_sha256": identity["source_sha256"],
            "derived_from_disk_sha256": identity["disk_sha256"],
            "disk_sha256": sha256_file(disk),
            "storage_format": STORAGE_FORMAT,
            "created_epoch": int(time.time()),
        }
        for disposable in (log, temporary_dir / "serial.log"):
            disposable.unlink(missing_ok=True)
        write_json(temporary_dir / "identity.json", derived_identity)
        self._seal(disk, temporary_dir / "identity.json")
        if target_dir.exists():
            shutil.rmtree(target_dir)
        os.replace(temporary_dir, target_dir)
        return derived_identity

    def _launch_linux(self, lane: Lane, disk: Path, seed: Path | None, runtime: Path):
        acceleration = "kvm" if os.access("/dev/kvm", os.R_OK | os.W_OK) else "tcg"
        log = runtime / "qemu-provision.log"
        stream = log.open("ab", buffering=0)
        command = [
            self.tool("qemu-system-x86_64"), "-name", f"sniptale-lab-{lane.name},process=sniptale-{lane.name}",
            "-machine", "q35", "-accel", acceleration if acceleration == "kvm" else "tcg,thread=multi", "-cpu", cpu_model(lane, acceleration),
            "-smp", str(lane.vcpus), "-m", str(lane.memory_mib), "-no-reboot",
            "-drive", f"if=none,id=system,file={disk},format=qcow2,cache=none,discard=unmap",
            "-device", "virtio-blk-pci,drive=system,addr=0x2",
            "-netdev", f"user,id=net0,hostfwd=tcp:127.0.0.1:{lane.ssh_port}-:22",
            "-device", "virtio-net-pci,netdev=net0,mac=52:54:00:12:34:56,addr=0x1", "-device", "virtio-vga,addr=0x3",
            "-device", "qemu-xhci", "-device", "usb-kbd", "-device", "usb-tablet",
            "-display", "none", "-vnc", f"127.0.0.1:{lane.vnc_display}",
            "-serial", f"file:{runtime / 'serial.log'}",
        ]
        if seed is not None:
            command[command.index("-netdev"):command.index("-netdev")] = [
                "-drive", f"file={seed},format=raw,media=cdrom,readonly=on",
            ]
        process = subprocess.Popen(
            command, stdin=subprocess.DEVNULL, stdout=stream, stderr=subprocess.STDOUT, start_new_session=True,
        )
        stream.close()
        return process, log, acceleration

    def _windows(self, lane: Lane) -> dict[str, object]:
        iso_value = os.environ.get("SNIPTALE_E2E_WINDOWS_ISO", "")
        checksum = os.environ.get("SNIPTALE_E2E_WINDOWS_ISO_SHA256", "")
        if not iso_value or not checksum:
            raise LabError(
                "Windows provisioning requires the official Windows 11 Enterprise Evaluation x64 ISO "
                "and its Microsoft-published SHA-256 via SNIPTALE_E2E_WINDOWS_ISO and "
                "SNIPTALE_E2E_WINDOWS_ISO_SHA256"
            )
        if not re.fullmatch(r"[0-9a-fA-F]{64}", checksum):
            raise IntegrityError("Windows ISO SHA-256 must be 64 hexadecimal characters")
        checksum = checksum.lower()
        provenance = OFFICIAL_WINDOWS_MEDIA.get(checksum)
        if provenance is None:
            raise IntegrityError("Windows ISO SHA-256 is not in the Microsoft-published Evaluation media allowlist")
        parsed = urllib.parse.urlparse(iso_value)
        if parsed.scheme:
            if parsed.scheme != "https" or parsed.hostname not in {
                "download.microsoft.com", "software-download.microsoft.com",
                "software-static.download.prss.microsoft.com",
            }:
                raise IntegrityError("Windows ISO URL must use an official Microsoft HTTPS download host")
            iso = self.paths.images / f"windows-11-enterprise-{checksum}.iso"
            if iso.exists():
                require_sha256(iso, checksum)
            else:
                download_https(iso_value, iso, checksum, timeout=7200)
        else:
            source_iso = Path(iso_value).expanduser().resolve(strict=True)
            if source_iso.stat().st_size > 16 * 1024 * 1024 * 1024:
                raise IntegrityError("Windows ISO exceeds the 16 GiB lab input limit")
            require_sha256(source_iso, checksum)
            iso = self.paths.images / f"windows-11-enterprise-{checksum}.iso"
            if iso.exists():
                require_sha256(iso, checksum)
            elif source_iso != iso:
                partial = iso.with_suffix(".iso.partial")
                shutil.copy2(source_iso, partial)
                require_sha256(partial, checksum)
                os.replace(partial, iso)
            else:
                iso = source_iso
        require_sha256(iso, checksum)
        target_dir = self.paths.golden / lane.name
        temporary_dir = self.paths.golden / f".{lane.name}.provisioning"
        if temporary_dir.exists():
            shutil.rmtree(temporary_dir)
        temporary_dir.mkdir(parents=True, mode=0o700)
        disk = temporary_dir / "disk.qcow2"
        answer_iso = temporary_dir / "answer.iso"
        self._windows_answer_iso(answer_iso)
        run([self.tool("qemu-img"), "create", "-q", "-f", "qcow2", str(disk), f"{lane.disk_gib}G"], timeout=120)
        process, log, acceleration = self._launch_windows(lane, disk, iso, answer_iso, temporary_dir)
        time.sleep(1)
        if process.poll() is not None:
            raise LabError(f"Windows provisioning QEMU failed to start: {log.read_text(encoding='utf-8', errors='replace')[-4096:]}")
        guest_identity: dict[str, object] | None = None
        try:
            boot_vm = RunningVm(
                lane.name, process.pid, disk, temporary_dir / "qmp.sock", int(time.time()),
                checksum, True,
            )
            manager = VmManager(self.paths, self.config)
            serial_log = temporary_dir / "serial.log"
            boot_deadline = time.monotonic() + 60
            while time.monotonic() < boot_deadline:
                if process.poll() is not None:
                    raise LabError("Windows provisioning QEMU exited before DVD boot")
                serial_tail = (
                    serial_log.read_text(encoding="utf-8", errors="replace")[-4096:]
                    if serial_log.exists() else ""
                )
                if 'starting Boot0001 "UEFI QEMU DVD-ROM' in serial_tail:
                    break
                time.sleep(0.1)
            else:
                raise TimeoutExpired("Windows provisioning did not reach the UEFI DVD boot prompt")
            for _ in range(12):
                manager.qmp(
                    boot_vm,
                    "send-key",
                    {"keys": [{"type": "qcode", "data": "spc"}]},
                )
                time.sleep(0.5)
            ssh = GuestSsh(self.paths, lane.ssh_port, "lab")
            ssh.forget_host_key()
            ssh.wait(
                self.config.timeouts.provision,
                probe=["cmd.exe", "/d", "/c", "exit", "0"],
            )
            deadline = time.monotonic() + self.config.timeouts.provision
            sentinel_seen = False
            while time.monotonic() < deadline:
                if process.poll() is not None:
                    break
                result = subprocess.run(
                    ["ssh", *ssh.options(), "lab@127.0.0.1", "--", "cmd.exe", "/c", "if exist C:\\SniptaleLab\\provisioned (exit 0) else (exit 1)"],
                    stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False,
                )
                if result.returncode == 0:
                    sentinel_seen = True
                    break
                time.sleep(15)
            if not sentinel_seen and process.poll() is None:
                raise TimeoutExpired("Windows provisioning timed out")
            from windows_session import require_license
            require_license(ssh, 14400)
            identity_output = ssh.command(powershell_encoded(
                "$p=Get-ItemProperty 'HKLM:\\SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion'; "
                "$e=(Get-WindowsEdition -Online).Edition; "
                "[ordered]@{edition=$e;display_version=$p.DisplayVersion;build=[int]$p.CurrentBuildNumber} | ConvertTo-Json -Compress"
            ), timeout=60).stdout.strip()
            try:
                candidate_identity = json.loads(identity_output)
            except json.JSONDecodeError as exc:
                raise IntegrityError("Windows guest identity probe returned invalid JSON") from exc
            if (
                not isinstance(candidate_identity, dict)
                or candidate_identity.get("edition") != provenance["edition"]
                or candidate_identity.get("display_version") != provenance["display_version"]
                or not isinstance(candidate_identity.get("build"), int)
                or candidate_identity["build"] < provenance["minimum_build"]
            ):
                raise IntegrityError("installed Windows guest does not match the approved Evaluation ISO identity")
            guest_identity = candidate_identity
            try:
                process.wait(timeout=self.config.timeouts.shutdown + 180)
            except subprocess.TimeoutExpired as exc:
                terminate_process(process.pid, timeout=5)
                raise TimeoutExpired("Windows golden did not power off") from exc
            if process.returncode != 0:
                tail = log.read_text(encoding="utf-8", errors="replace")[-4096:]
                raise LabError(f"Windows provisioning QEMU failed: {tail}")
        except BaseException:
            if process.poll() is None:
                terminate_process(process.pid, timeout=5)
            raise
        compact_unsealed_disk(self.paths, disk)
        identity = {
            "schema_version": 1,
            "lane": lane.name,
            "os": "windows-11-enterprise-evaluation",
            "display_server": "win32",
            "provision_acceleration": acceleration,
            "source_sha256": checksum,
            "source_release": provenance["release"],
            "source_provenance": "microsoft-evaluation-center-published-sha256",
            "disk_sha256": sha256_file(disk),
            "storage_format": STORAGE_FORMAT,
            "firmware_code_sha256": sha256_file(temporary_dir / "OVMF_CODE.fd"),
            "firmware_vars_sha256": sha256_file(temporary_dir / "OVMF_VARS.fd"),
            "guest_edition": guest_identity["edition"],
            "guest_display_version": guest_identity["display_version"],
            "guest_build": guest_identity["build"],
            "created_epoch": int(time.time()),
        }
        write_json(temporary_dir / "identity.json", identity)
        for disposable in (answer_iso, temporary_dir / "answer", log, temporary_dir / "serial.log"):
            if disposable.is_dir():
                shutil.rmtree(disposable)
            else:
                disposable.unlink(missing_ok=True)
        self._seal(
            disk,
            temporary_dir / "identity.json",
            temporary_dir / "OVMF_CODE.fd",
            temporary_dir / "OVMF_VARS.fd",
        )
        if target_dir.exists():
            shutil.rmtree(target_dir)
        os.replace(temporary_dir, target_dir)
        return identity

    def _windows_answer_iso(self, destination: Path) -> None:
        seed = destination.parent / "answer"
        seed.mkdir(mode=0o700)
        public_key = (self.paths.cache / "ssh/lab_ed25519.pub").read_text(encoding="utf-8").strip()
        source = self.paths.root / "tooling/desktop/e2e_lab/guest/windows"
        shutil.copy2(source / "Autounattend.xml", seed / "Autounattend.xml")
        script = (source / "provision.ps1").read_text(encoding="utf-8").replace("{{SSH_PUBLIC_KEY}}", public_key)
        versions = read_json(self.paths.root / "tooling/configs/qa/tool-versions.json")
        for token, version in {"RUST_VERSION": versions["rust"]["channel"], "WIX_VERSION": versions["windows"]["wix"]}.items():
            if not isinstance(version, str) or re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", version) is None:
                raise IntegrityError("invalid provisioned build tool version")
            script = script.replace("{{" + token + "}}", version)
        (seed / "provision.ps1").write_text(script, encoding="utf-8-sig")
        run(
            [self.tool("genisoimage"), "-quiet", "-output", str(destination), "-volid", "SNIPTALE_E2E", "-joliet", "-rock", str(seed / "Autounattend.xml"), str(seed / "provision.ps1")],
            timeout=120,
        )

    def _launch_windows(self, lane: Lane, disk: Path, install_iso: Path, answer_iso: Path, runtime: Path):
        acceleration = "kvm" if os.access("/dev/kvm", os.R_OK | os.W_OK) else "tcg"
        log = runtime / "qemu-provision.log"
        command = [
            self.tool("qemu-system-x86_64"), "-name", f"sniptale-lab-{lane.name},process=sniptale-{lane.name}",
            "-machine", "q35", "-accel", acceleration if acceleration == "kvm" else "tcg,thread=multi", "-cpu", cpu_model(lane, acceleration),
            "-smp", str(lane.vcpus), "-m", str(lane.memory_mib), "-boot", "order=d,once=d",
            "-drive", f"if=none,id=system,file={disk},format=qcow2,cache=none,discard=unmap",
            "-device", "nvme,drive=system,serial=sniptalelab,addr=0x2",
            "-drive", f"file={install_iso},media=cdrom,readonly=on",
            "-drive", f"file={answer_iso},format=raw,media=cdrom,readonly=on",
            "-netdev", f"user,id=net0,hostfwd=tcp:127.0.0.1:{lane.ssh_port}-:22",
            "-device", "e1000e,netdev=net0,mac=52:54:00:12:34:56,addr=0x1", "-device", "VGA,addr=0x3", "-device", "qemu-xhci",
            "-device", "usb-kbd", "-device", "usb-tablet", "-audiodev", "driver=none,id=audio0",
            "-device", "ich9-intel-hda", "-device", "hda-duplex,audiodev=audio0",
            "-display", "none", "-vnc", f"127.0.0.1:{lane.vnc_display}",
            "-qmp", f"unix:{runtime / 'qmp.sock'},server=on,wait=off",
            "-serial", f"file:{runtime / 'serial.log'}",
        ]
        VmManager(self.paths, self.config)._append_windows_firmware(command, runtime)
        stream = log.open("ab", buffering=0)
        process = subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=stream, stderr=subprocess.STDOUT, start_new_session=True)
        stream.close()
        return process, log, acceleration
