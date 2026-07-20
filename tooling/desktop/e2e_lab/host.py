"""Host bootstrap and read-only doctor checks."""

from __future__ import annotations

import fcntl
import os
import re
import shutil
import socket
import subprocess
import urllib.parse
from dataclasses import dataclass
from pathlib import Path

from config import LabConfig
from errors import IntegrityError, LabError
from paths import LabPaths
from system import download_https, validate_resources
from limits import directory_allocated_bytes, directory_bytes

HOST_PACKAGES = (
    "qemu-system-x86", "qemu-utils", "ovmf", "swtpm", "swtpm-tools", "genisoimage",
)
HOST_COMMANDS = ("ssh", "scp", "ssh-keygen", "tar", "sha256sum")


@dataclass(frozen=True)
class Check:
    name: str
    passed: bool
    detail: str
    required: bool = True

    def as_dict(self) -> dict[str, object]:
        return {"name": self.name, "passed": self.passed, "required": self.required, "detail": self.detail}


def _host_tool(paths: LabPaths, name: str) -> str | None:
    return shutil.which(name) or (str(paths.cache / f"host-tools/bin/{name}") if (paths.cache / f"host-tools/bin/{name}").exists() else None)


def _port_available(port: int) -> bool:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        try:
            sock.bind(("127.0.0.1", port))
        except OSError:
            return False
    return True


def _kvm_access() -> tuple[bool, str]:
    try:
        descriptor = os.open("/dev/kvm", os.O_RDWR | os.O_CLOEXEC)
    except OSError as exc:
        return False, f"KVM device cannot be opened by current user: {exc.strerror}"
    try:
        api_version = fcntl.ioctl(descriptor, 0xAE00, 0)
    except OSError as exc:
        return False, f"KVM API probe failed: {exc.strerror}"
    finally:
        os.close(descriptor)
    return api_version == 12, f"KVM API version {api_version}"


def doctor(paths: LabPaths, config: LabConfig) -> list[Check]:
    kvm_passed, kvm_detail = _kvm_access()
    checks = [
        Check("wsl2", "microsoft-standard-WSL2" in os.uname().release, os.uname().release),
        Check("systemd", Path("/run/systemd/system").is_dir(), "systemd runtime directory"),
        Check("kvm-device", Path("/dev/kvm").exists(), "/dev/kvm present"),
        Check("kvm-access", kvm_passed, kvm_detail),
    ]
    for command in HOST_COMMANDS:
        checks.append(Check(f"command:{command}", shutil.which(command) is not None, shutil.which(command) or "missing"))
    for command in ("qemu-system-x86_64", "qemu-img", "genisoimage"):
        value = _host_tool(paths, command)
        checks.append(Check(f"command:{command}", value is not None, value or "missing"))
    try:
        resources = validate_resources(
            paths.lab if paths.lab.exists() else paths.root,
            min_free_gib=config.limits.min_free_gib,
            min_memory_mib=config.limits.min_available_memory_mib,
        )
        checks.append(Check("resources", True, f"{resources['free_disk_gib']} GiB free, {resources['available_memory_mib']} MiB available RAM"))
    except LabError as exc:
        checks.append(Check("resources", False, str(exc)))
    cache_gib = directory_bytes(paths.cache) / (1024 ** 3)
    checks.append(Check(
        "cache-limit",
        cache_gib <= config.limits.max_cache_gib,
        f"{cache_gib:.2f} GiB of {config.limits.max_cache_gib} GiB",
    ))
    temporary_gib = directory_allocated_bytes(paths.lab) / (1024 ** 3)
    persistent_gib = directory_allocated_bytes(paths.artifacts) / (1024 ** 3)
    checks.extend((
        Check(
            "temporary-limit", temporary_gib <= config.limits.max_temporary_gib,
            f"{temporary_gib:.2f} GiB of {config.limits.max_temporary_gib} GiB",
        ),
        Check(
            "persistent-limit", persistent_gib <= config.limits.max_persistent_gib,
            f"{persistent_gib:.2f} GiB of {config.limits.max_persistent_gib} GiB",
        ),
    ))
    for lane in config.lanes.values():
        checks.append(Check(f"loopback:ssh:{lane.name}", _port_available(lane.ssh_port), f"127.0.0.1:{lane.ssh_port}"))
        checks.append(Check(f"loopback:vnc:{lane.name}", _port_available(5900 + lane.vnc_display), f"127.0.0.1:{5900 + lane.vnc_display}"))
    return checks


def _apt_dependencies(packages: tuple[str, ...]) -> list[str]:
    command = [
        "apt-cache", "depends", "--recurse", "--no-recommends", "--no-suggests",
        "--no-conflicts", "--no-breaks", "--no-replaces", "--no-enhances", *packages,
    ]
    result = subprocess.run(command, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=False)
    if result.returncode != 0:
        raise LabError(f"could not resolve host packages: {result.stderr.strip()}")
    names = set(packages)
    for line in result.stdout.splitlines():
        match = re.match(r"\s*(?:Pre)?Depends:\s+([^ <][^ ]*)", line)
        if match:
            names.add(match.group(1).split(":", 1)[0])
    return sorted(names)


def _installed(package: str) -> bool:
    result = subprocess.run(
        ["dpkg-query", "-W", "-f=${db:Status-Abbrev}", package], text=True,
        stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, check=False,
    )
    return result.returncode == 0 and result.stdout.startswith("ii ")


def _download_deb(package: str, destination: Path) -> Path:
    result = subprocess.run(
        ["apt-get", "--print-uris", "download", package], text=True,
        stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=False,
    )
    if result.returncode != 0:
        raise LabError(f"could not resolve official Ubuntu package {package}: {result.stderr.strip()}")
    match = re.search(r"'([^']+\.deb)'\s+(\S+)\s+\d+\s+\S+:[0-9a-fA-F]+", result.stdout)
    if not match:
        raise IntegrityError(f"apt did not provide a repository-bound URI for {package}")
    url, filename = match.groups()
    parsed = urllib.parse.urlparse(url)
    if parsed.hostname not in {"archive.ubuntu.com", "security.ubuntu.com"}:
        raise IntegrityError(f"unexpected Ubuntu package host for {package}")
    repository_path = urllib.parse.unquote(parsed.path).lstrip("/")
    if repository_path.startswith("ubuntu/"):
        repository_path = repository_path.removeprefix("ubuntu/")
    metadata = subprocess.run(
        ["apt-cache", "show", package], text=True, stdout=subprocess.PIPE,
        stderr=subprocess.PIPE, check=False,
    )
    checksum = ""
    for stanza in metadata.stdout.split("\n\n"):
        fields = dict(
            line.split(": ", 1) for line in stanza.splitlines() if ": " in line
        )
        if fields.get("Filename") == repository_path:
            checksum = fields.get("SHA256", "")
            break
    if len(checksum) != 64:
        raise IntegrityError(f"signed apt metadata did not provide SHA-256 for {package}")
    secure_url = urllib.parse.urlunparse(parsed._replace(scheme="https"))
    return download_https(secure_url, destination / urllib.parse.unquote(filename), checksum)


def _write_wrapper(prefix: Path, name: str) -> None:
    target = prefix / f"usr/bin/{name}"
    if not target.exists():
        return
    wrapper = prefix / f"bin/{name}"
    wrapper.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    extra = '"-L" "$PREFIX/usr/share/qemu" ' if name.startswith("qemu-system") else ""
    wrapper.write_text(
        "#!/usr/bin/env bash\n"
        "set -euo pipefail\n"
        "PREFIX=\"$(cd -- \"$(dirname -- \"${BASH_SOURCE[0]}\")/..\" && pwd -P)\"\n"
        "export LD_LIBRARY_PATH=\"$PREFIX/usr/lib/x86_64-linux-gnu:$PREFIX/lib/x86_64-linux-gnu:${LD_LIBRARY_PATH:-}\"\n"
        "export QEMU_MODULE_DIR=\"$PREFIX/usr/lib/x86_64-linux-gnu/qemu\"\n"
        f"exec \"$PREFIX/usr/bin/{name}\" {extra}\"$@\"\n",
        encoding="utf-8",
    )
    wrapper.chmod(0o700)


def bootstrap(paths: LabPaths) -> dict[str, object]:
    paths.ensure()
    prefix = paths.cache / "host-tools"
    archives = paths.cache / "apt-archives"
    archives.mkdir(parents=True, exist_ok=True, mode=0o700)
    if not all(_host_tool(paths, name) for name in ("qemu-system-x86_64", "qemu-img", "genisoimage")):
        prefix.mkdir(parents=True, exist_ok=True, mode=0o700)
        downloaded = 0
        for package in _apt_dependencies(HOST_PACKAGES):
            if _installed(package):
                continue
            try:
                deb = _download_deb(package, archives)
            except LabError:
                if package in HOST_PACKAGES:
                    raise
                continue
            result = subprocess.run(["dpkg-deb", "-x", str(deb), str(prefix)], check=False)
            if result.returncode != 0:
                raise LabError(f"could not extract host package {package}")
            downloaded += 1
    for name in ("qemu-system-x86_64", "qemu-img", "genisoimage", "swtpm"):
        _write_wrapper(prefix, name)
    qemu_data = prefix / "usr/share/qemu"
    qemu_data.mkdir(parents=True, exist_ok=True)
    for firmware_dir in (prefix / "usr/share/seabios", prefix / "usr/lib/ipxe/qemu"):
        if not firmware_dir.is_dir():
            continue
        for firmware in firmware_dir.iterdir():
            link = qemu_data / firmware.name
            if link.is_symlink() and link.resolve(strict=False) != firmware.resolve(strict=False):
                link.unlink()
            if not link.exists() and not link.is_symlink():
                link.symlink_to(firmware)
    key = paths.cache / "ssh/lab_ed25519"
    if not key.exists():
        key.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
        result = subprocess.run(
            ["ssh-keygen", "-q", "-t", "ed25519", "-N", "", "-C", "sniptale-e2e-lab", "-f", str(key)],
            check=False,
        )
        if result.returncode != 0:
            raise LabError("could not create the lab SSH key")
    key.chmod(0o600)
    key.with_suffix(".pub").chmod(0o644)
    available = {name: _host_tool(paths, name) for name in ("qemu-system-x86_64", "qemu-img", "genisoimage")}
    missing = [name for name, value in available.items() if value is None]
    if missing:
        raise LabError(f"bootstrap did not provide required commands: {', '.join(missing)}")
    return {"host_tools": available, "ssh_public_key": key.with_suffix(".pub").read_text(encoding="utf-8").strip()}
