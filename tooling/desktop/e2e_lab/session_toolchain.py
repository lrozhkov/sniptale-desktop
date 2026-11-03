"""Prepare an official, pinned Windows Rust toolchain for offline guest compilation."""

from __future__ import annotations

import shutil
import tarfile
import re
from pathlib import PurePosixPath

from errors import IntegrityError
from ssh import powershell_encoded
from state import read_json, write_json
from system import download_https, require_sha256, sha256_file


def prepare_toolchain(paths):
    lock = read_json(paths.root / "tooling/configs/qa/toolchain-lock.json")
    policy = dict(lock.get("rust", {})) if isinstance(lock, dict) else None
    if policy is not None:
        policy.pop("publishedAt", None)
        policy.pop("source", None)
    versions = read_json(paths.root / "tooling/configs/qa/tool-versions.json")
    if (
        policy is None
        or set(policy) != {"schema_version", "rust_version", "target", "manifest_sha256", "components"}
        or type(policy.get("schema_version")) is not int
        or policy["schema_version"] != 1
        or not isinstance(policy.get("rust_version"), str)
        or re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", policy["rust_version"]) is None
    ):
        raise IntegrityError("invalid offline Rust toolchain policy")
    if (
        versions is None
        or policy.get("rust_version") != versions.get("rust", {}).get("channel")
        or policy.get("target") != "x86_64-pc-windows-msvc"
    ):
        raise IntegrityError("offline Rust toolchain differs from the pinned compiler policy")
    if (
        not isinstance(policy["manifest_sha256"], str)
        or re.fullmatch(r"[0-9a-f]{64}", policy["manifest_sha256"]) is None
        or not isinstance(policy["components"], list)
        or len(policy["components"]) != 3
    ):
        raise IntegrityError("invalid compiler component inventory")
    names = set()
    for component in policy["components"]:
        if (
            not isinstance(component, dict)
            or set(component) != {"name", "url", "sha256"}
            or not isinstance(component["name"], str)
            or component["name"] not in {"rustc", "cargo", "rust-std"}
            or component["name"] in names
        ):
            raise IntegrityError("invalid compiler component identity")
        names.add(component["name"])
        if (
            not isinstance(component["sha256"], str)
            or re.fullmatch(r"[0-9a-f]{64}", component["sha256"]) is None
            or not isinstance(component["url"], str)
            or re.fullmatch(
                r"https://static\.rust-lang\.org/dist/[0-9]{4}-[0-9]{2}-[0-9]{2}/"
                + re.escape(component["name"] + "-" + policy["rust_version"] + "-" + policy["target"])
                + r"\.tar\.xz",
                component["url"],
            )
            is None
        ):
            raise IntegrityError("invalid compiler component provenance")
    cache = paths.scoped(paths.cache / "session-dependencies" / ("rust-" + policy["rust_version"]))
    cache.mkdir(parents=True, exist_ok=True)
    target = cache / "windows-rust.tar.gz"
    if target.is_symlink():
        raise IntegrityError("compiler cache is redirected")
    stamp = read_json(cache / "toolchain.json")
    if stamp and stamp.get("policy") == policy and target.is_file() and sha256_file(target) == stamp.get("sha256"):
        return target, stamp
    stage = cache / "stage"
    if stage.exists():
        shutil.rmtree(paths.scoped(stage))
    stage.mkdir()
    files = 0
    total = 0
    try:
        for component in policy["components"]:
            archive = cache / (component["name"] + ".tar.xz")
            if not archive.exists():
                download_https(component["url"], archive, component["sha256"], timeout=300, max_bytes=256 * 1024 * 1024)
            require_sha256(archive, component["sha256"])
            with tarfile.open(archive, "r:xz") as bundle:
                for member in bundle:
                    parts = PurePosixPath(member.name).parts
                    if member.isdir() or len(parts) < 3 or not parts[1].startswith(component["name"]):
                        continue
                    relative = PurePosixPath(*parts[2:])
                    if relative.is_absolute() or ".." in relative.parts or not member.isfile():
                        raise IntegrityError("invalid compiler package path or entry type")
                    files += 1
                    total += member.size
                    if files > 20000 or total > 2 * 1024**3 or member.size > 256 * 1024**2:
                        raise IntegrityError("compiler package extraction budget exceeded")
                    destination = stage / relative
                    destination.parent.mkdir(parents=True, exist_ok=True)
                    source = bundle.extractfile(member)
                    if source is None:
                        raise IntegrityError("compiler component member missing")
                    with source, destination.open("wb") as output:
                        shutil.copyfileobj(source, output)
        rustc = sha256_file(stage / "bin/rustc.exe")
        cargo = sha256_file(stage / "bin/cargo.exe")
        with tarfile.open(target, "w:gz", compresslevel=1) as archive:
            archive.add(stage, arcname="rust-toolchain")
        stamp = {"policy": policy, "sha256": sha256_file(target), "rustc_sha256": rustc, "cargo_sha256": cargo}
        write_json(cache / "toolchain.json", stamp)
        return target, stamp
    finally:
        shutil.rmtree(paths.scoped(stage))


def upload_toolchain(paths, ssh):
    archive, stamp = prepare_toolchain(paths)
    existing = ssh.command(
        powershell_encoded(
            "$ErrorActionPreference='Stop'; $ProgressPreference='SilentlyContinue'; "
            "$root='C:\\SniptaleLab\\rust-toolchain'; "
            "if((Test-Path $root\\bin\\rustc.exe) -and (Test-Path $root\\bin\\cargo.exe)){"
            f"if((Get-FileHash $root\\bin\\rustc.exe).Hash.ToLowerInvariant() -eq '{stamp['rustc_sha256']}' -and "
            f"(Get-FileHash $root\\bin\\cargo.exe).Hash.ToLowerInvariant() -eq '{stamp['cargo_sha256']}'){{Write-Output 'compiler-ready'}}}}"
        ),
        timeout=30,
    ).stdout.strip()
    if existing != "compiler-ready":
        ssh.upload(archive, "windows-rust.tar.gz", timeout=300)
        ssh.command(
            powershell_encoded(
                "$ErrorActionPreference='Stop'; $ProgressPreference='SilentlyContinue'; "
                f"if((Get-FileHash windows-rust.tar.gz).Hash.ToLowerInvariant() -ne '{stamp['sha256']}'){{throw 'compiler bundle hash mismatch'}}; "
                "if(Test-Path C:\\SniptaleLab\\rust-toolchain){Remove-Item C:\\SniptaleLab\\rust-toolchain -Recurse -Force}; "
                "tar.exe -xzf windows-rust.tar.gz -C C:\\SniptaleLab; if($LASTEXITCODE -ne 0){throw 'compiler extraction failed'}; "
                "Remove-Item windows-rust.tar.gz -Force"
            ),
            timeout=300,
        )
    return stamp
