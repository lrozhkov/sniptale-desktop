"""Install verified script-checking tools in a repository-local cache."""

from __future__ import annotations

import gzip
import hashlib
import io
import json
import os
import sys
import shutil
import subprocess
import tarfile
import tempfile
import urllib.request
import zipfile
from pathlib import Path

from .repository import ROOT
from .toolchain import check

CACHE = ROOT / ".tmp/qa-tools/script-tools"


def run(arguments: list[str], *, cwd: Path | None = None) -> None:
    subprocess.run(arguments, cwd=cwd or ROOT, check=True, timeout=900)


def bounded_members(package: tarfile.TarFile, maximum: int) -> list[tarfile.TarInfo]:
    members = []
    total = 0
    for member in package:
        total += member.size
        if len(members) >= 50000 or total > maximum:
            raise ValueError("script tool extraction exceeds member or byte budget")
        members.append(member)
    return members


def python_identity(root: Path) -> str:
    config = root / "tooling/configs/qa"
    return hashlib.sha256(
        (config / "script-python-requirements.txt").read_bytes()
        + (config / "script-python-build-requirements.txt").read_bytes()
        + sys.version.encode()
    ).hexdigest()


def distribution_identity(spec: dict) -> str:
    return hashlib.sha256(
        json.dumps(
            {key: spec[key] for key in ("version", "manager", "repository", "tag", "linuxX64")}, sort_keys=True
        ).encode()
    ).hexdigest()


def verify_installed() -> None:
    # Canonical metadata admission is a separate preceding wrapper step.
    config = ROOT / "tooling/configs/qa"
    lock = json.loads((config / "toolchain-lock.json").read_text())
    expected = {
        CACHE / "python/.identity": python_identity(ROOT),
        CACHE / "npm/.identity": hashlib.sha256((config / "script-node/package-lock.json").read_bytes()).hexdigest(),
    }
    for name, spec in lock["scriptTools"].items():
        if spec["manager"] == "github":
            target = CACHE / (name + "-" + spec["version"])
            expected[target / ".identity"] = distribution_identity(spec)
            if not (target / ("PSScriptAnalyzer.psd1" if name == "PSScriptAnalyzer" else name)).is_file():
                raise ValueError("installed checker is missing; run install-script-tools.sh")
    for marker, identity in expected.items():
        if not marker.is_file() or marker.read_text() != identity:
            raise ValueError("installed checker inputs differ from catalog; run install-script-tools.sh")
    for name in ("python", "ruff", "yamllint", "zizmor"):
        if not (CACHE / "python/bin" / name).is_file():
            raise ValueError("installed Python checker is missing; run install-script-tools.sh")
    for name in ("eslint", "pyright"):
        if not (CACHE / "npm/node_modules/.bin" / name).is_file():
            raise ValueError("installed npm checker is missing; run install-script-tools.sh")
    schema = lock["scriptTools"]["renovate"]
    schema_path = CACHE / ("renovate-schema-" + schema["version"]) / "renovate-schema.json"
    if hashlib.sha256(schema_path.read_bytes()).hexdigest() != schema["schemaAsset"]["schemaSha256"]:
        raise ValueError("installed Renovate schema identity mismatch")
    print("OK installed checker identities match cataloged artifacts and dependency locks")


def install() -> None:
    catalog = check()
    lock = json.loads((ROOT / "tooling/configs/qa/toolchain-lock.json").read_text())
    CACHE.mkdir(parents=True, exist_ok=True)
    node = lock["browserAutomation"]["node"]
    node_target = ROOT / ".tmp/qa-tools" / ("node-" + node["version"])
    if not (node_target / "bin/node").is_file():
        asset = node["linuxX64"]
        expected = f"https://nodejs.org/dist/v{node['version']}/node-v{node['version']}-linux-x64.tar.xz"
        if asset["url"] != expected:
            raise ValueError("unexpected Linux Node source")
        with tempfile.TemporaryDirectory(dir=CACHE, prefix="node-") as directory:
            archive = Path(directory) / "node.tar.xz"
            with urllib.request.urlopen(expected, timeout=90) as response:
                body = response.read(80 * 1024 * 1024 + 1)
            if len(body) > 80 * 1024 * 1024 or hashlib.sha256(body).hexdigest() != asset["sha256"]:
                raise ValueError("Linux Node checksum mismatch")
            archive.write_bytes(body)
            with tarfile.open(archive) as package:
                members = bounded_members(package, 500 * 1024 * 1024)
                package.extractall(directory, members=members, filter="data")
            (Path(directory) / f"node-v{node['version']}-linux-x64").rename(node_target)
    os.environ["PATH"] = str(node_target / "bin") + os.pathsep + os.environ["PATH"]
    if (
        subprocess.run(["node", "--version"], capture_output=True, text=True, check=True, timeout=10).stdout.strip()
        != "v" + node["version"]
    ):
        raise ValueError("script tooling Node runtime differs from catalog")
    for name, spec in catalog["scriptTools"].items():
        if spec["manager"] != "github":
            continue
        spec = lock["scriptTools"][name]
        target = CACHE / (name + "-" + spec["version"])
        identity = distribution_identity(spec)
        if (target / ".identity").is_file() and (target / ".identity").read_text() == identity:
            continue
        asset = spec["linuxX64"]
        expected_prefix = f"https://github.com/{spec['repository']}/releases/download/{spec['tag']}/"
        if not asset["url"].startswith(expected_prefix):
            raise ValueError("script tool asset differs from approved release")
        with tempfile.TemporaryDirectory(dir=CACHE, prefix="install-") as directory:
            staging = Path(directory)
            archive = staging / "asset"
            request = urllib.request.Request(asset["url"], headers={"User-Agent": "SniptaleToolchain/1"})
            with urllib.request.urlopen(request, timeout=90) as response, archive.open("wb") as output:
                remaining = 180 * 1024 * 1024
                while block := response.read(min(remaining + 1, 1024 * 1024)):
                    remaining -= len(block)
                    if remaining < 0:
                        raise ValueError("script tool archive exceeds budget")
                    output.write(block)
            if hashlib.sha256(archive.read_bytes()).hexdigest() != asset["sha256"]:
                raise ValueError("script tool archive checksum mismatch")
            destination = staging / "out"
            destination.mkdir()
            if name in {"shellcheck", "actionlint", "pwsh"}:
                with tarfile.open(archive) as package:
                    members = bounded_members(package, 700 * 1024 * 1024)
                    package.extractall(destination, members=members, filter="data")
                if name == "shellcheck":
                    shutil.move(str(next(destination.glob("*/shellcheck"))), destination / "shellcheck")
            elif name == "PSScriptAnalyzer":
                with zipfile.ZipFile(archive) as package:
                    if (
                        len(package.infolist()) > 50000
                        or sum(item.file_size for item in package.infolist()) > 500 * 1024 * 1024
                    ):
                        raise ValueError("script module extraction exceeds budget")
                    for item in package.infolist():
                        if Path(item.filename).is_absolute() or ".." in Path(item.filename).parts:
                            raise ValueError("unsafe script module path")
                    package.extractall(destination)
            else:
                executable = destination / name
                if name == "taplo":
                    with gzip.GzipFile(fileobj=io.BytesIO(archive.read_bytes())) as compressed:
                        body = compressed.read(100 * 1024 * 1024 + 1)
                    if len(body) > 100 * 1024 * 1024:
                        raise ValueError("Taplo extraction exceeds budget")
                else:
                    body = archive.read_bytes()
                executable.write_bytes(body)
            if name != "PSScriptAnalyzer":
                (destination / name).chmod(0o755)
            (destination / ".identity").write_text(identity)
            if target.exists():
                shutil.rmtree(target)
            destination.rename(target)
    schema = lock["scriptTools"]["renovate"]
    schema_dir = CACHE / ("renovate-schema-" + schema["version"])
    schema_path = schema_dir / "renovate-schema.json"
    expected = schema["schemaAsset"]
    if not schema_path.is_file() or hashlib.sha256(schema_path.read_bytes()).hexdigest() != expected["schemaSha256"]:
        from .script_toolchain import schema_files

        with urllib.request.urlopen(expected["url"], timeout=90) as response:
            body = response.read(64 * 1024 * 1024 + 1)
        if len(body) > 64 * 1024 * 1024 or hashlib.sha256(body).hexdigest() != expected["sha256"]:
            raise ValueError("Renovate schema distribution checksum mismatch")
        files = schema_files(body)
        if hashlib.sha256(files["renovate-schema.json"]).hexdigest() != expected["schemaSha256"]:
            raise ValueError("Renovate schema file checksum mismatch")
        schema_dir.mkdir(exist_ok=True)
        for name, content in files.items():
            (schema_dir / name).write_bytes(content)
    python_dir = CACHE / "python"
    python_lock = ROOT / "tooling/configs/qa/script-python-requirements.txt"
    build_lock = ROOT / "tooling/configs/qa/script-python-build-requirements.txt"
    python_id = python_identity(ROOT)
    if not (python_dir / ".identity").is_file() or (python_dir / ".identity").read_text() != python_id:
        run([sys.executable, "-m", "venv", str(python_dir)])
        pip = [
            str(python_dir / "bin/python"),
            "-m",
            "pip",
            "install",
            "--disable-pip-version-check",
            "--require-hashes",
            "--no-build-isolation",
        ]
        run([*pip, "-r", str(build_lock)])
        run([*pip, "-r", str(python_lock)])
        (python_dir / ".identity").write_text(python_id)
    npm_dir = CACHE / "npm"
    npm_dir.mkdir(exist_ok=True)
    for name in ("package.json", "package-lock.json"):
        shutil.copyfile(ROOT / "tooling/configs/qa/script-node" / name, npm_dir / name)
    npm_id = hashlib.sha256((npm_dir / "package-lock.json").read_bytes()).hexdigest()
    if not (npm_dir / ".identity").is_file() or (npm_dir / ".identity").read_text() != npm_id:
        run(["npm", "ci", "--ignore-scripts", "--no-audit", "--no-fund"], cwd=npm_dir)
        (npm_dir / ".identity").write_text(npm_id)
    print("OK verified script tool installation")


if __name__ == "__main__":
    import argparse

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check-installed", action="store_true")
    arguments = parser.parse_args()
    if arguments.check_installed:
        verify_installed()
    else:
        install()
