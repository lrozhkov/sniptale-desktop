"""Install verified Actions Python on Ubuntu 24.04 x64 without compilation."""

from __future__ import annotations

import hashlib
import os
import platform
import shutil
import subprocess
import sys
import tarfile
import tempfile
import urllib.request
from pathlib import Path

from .advisories import decode
from .repository import ROOT
from .toolchain import check


def runtime_environment(prefix: Path) -> dict[str, str]:
    return {
        **os.environ,
        "LD_LIBRARY_PATH": str(prefix / "lib")
        + (os.pathsep + os.environ["LD_LIBRARY_PATH"] if os.environ.get("LD_LIBRARY_PATH") else ""),
    }


def available(executable: Path, prefix: Path, version: str) -> bool:
    if not executable.is_file():
        return False
    result = subprocess.run(
        [
            str(executable),
            "-I",
            "-c",
            "import bz2,ctypes,ensurepip,json,lzma,platform,sqlite3,ssl,tomllib,venv,zlib; print(platform.python_version())",
        ],
        env=runtime_environment(prefix),
        capture_output=True,
        text=True,
        check=False,
        timeout=20,
    )
    return result.returncode == 0 and result.stdout.strip() == version


def install() -> None:
    catalog = check()
    version = catalog["python"]["version"]
    if platform.python_version() == version and available(Path(sys.executable), Path(sys.prefix), version):
        print("OK cataloged Python runtime is active")
        return
    if platform.system() != "Linux" or platform.machine() not in {"x86_64", "AMD64"}:
        raise ValueError("local Python bootstrap supports Linux x86_64; use the pinned setup-python input on CI")
    target = ROOT / ".tmp/qa-tools" / ("python-" + version)
    executable = target / "bin/python3"
    if available(executable, target, version):
        print("OK cached cataloged Python runtime")
        return
    source = decode((ROOT / "tooling/configs/qa/toolchain-lock.json").read_bytes())["python"]["linuxX64"]
    from .python_toolchain import validate

    validate(version, source)
    distribution = platform.freedesktop_os_release()
    if distribution.get("ID") != "ubuntu" or distribution.get("VERSION_ID") != "24.04":
        raise ValueError("local Python distribution requires Ubuntu 24.04 x64; use the pinned setup-python input on CI")
    target.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=target.parent, prefix="python-bootstrap-") as directory:
        work = Path(directory)
        archive = work / "python.tar.gz"
        with urllib.request.urlopen(source["url"], timeout=90) as response:
            body = response.read(128 * 1024 * 1024 + 1)
        if len(body) > 128 * 1024 * 1024 or hashlib.sha256(body).hexdigest() != source["sha256"]:
            raise ValueError("Python bootstrap source checksum mismatch")
        archive.write_bytes(body)
        installed = work / "payload"
        installed.mkdir()
        with tarfile.open(archive) as package:
            members = []
            total = 0
            for member in package:
                total += member.size
                if len(members) >= 100000 or member.size < 0 or total > 2 * 1024**3:
                    raise ValueError("Python bootstrap extraction exceeds budget")
                members.append(member)
            package.extractall(installed, members=members, filter="data")
        if not available(installed / "bin/python3", installed, version):
            raise ValueError("verified Python distribution lacks required standard-library modules")
        if target.exists():
            shutil.rmtree(target)
        installed.rename(target)
    if not available(executable, target, version):
        raise ValueError("Python distribution is missing required shared libraries on the current host")
    print("OK verified cataloged Python bootstrap")


if __name__ == "__main__":
    install()
