"""Install the pinned Linux analysis tools from verified upstream archives."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import re
import shutil
import stat
import subprocess
import tarfile
import tempfile
import urllib.parse
import urllib.request
from pathlib import Path

from .repository import ROOT
from .storage import atomic_write_json
from .toolchain import check


LIMIT = 4 * 1024**3


def sha256(path: Path) -> str:
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def tree_digest(root: Path) -> str:
    digest = hashlib.sha256()
    for path in sorted(root.rglob("*")):
        if path.is_symlink():
            if not path.resolve().is_relative_to(root.resolve()):
                raise ValueError("analysis tool symlink escapes installation")
            value = os.readlink(path)
        elif path.is_file():
            value = sha256(path)
        else:
            continue
        digest.update((path.relative_to(root).as_posix() + "\0" + value + "\0").encode())
    return digest.hexdigest()


def archive_members(archive: tarfile.TarFile) -> None:
    members = archive.getmembers()
    if len(members) > 100_000 or sum(member.size for member in members) > LIMIT:
        raise ValueError("analysis archive exceeds extraction budget")
    if any(member.isdev() or member.isfifo() for member in members):
        raise ValueError("analysis archive contains a device or FIFO")


def extract_archive(path: Path, destination: Path) -> None:
    with tarfile.open(path, "r:*") as bundle:
        archive_members(bundle)
        bundle.extractall(destination, filter="data")


def installation_base(name: str, root: Path) -> Path:
    if name != "codeql":
        return root / ".tmp/qa-tools/static-analysis"
    # Bundled externs must remain outside the CodeQL application source root.
    base = Path(tempfile.gettempdir()) / f"sniptale-codeql-tools-{os.getuid()}"
    if base.resolve().is_relative_to(root.resolve()):
        raise ValueError("CodeQL tool cache must remain outside the source root")
    base.mkdir(mode=0o700, exist_ok=True)
    metadata = base.lstat()
    if (
        base.is_symlink()
        or not stat.S_ISDIR(metadata.st_mode)
        or metadata.st_uid != os.getuid()
        or metadata.st_mode & 0o077
    ):
        raise ValueError("CodeQL tool cache is not a private owned directory")
    return base


def install(name: str, root: Path = ROOT) -> Path:
    if platform.system() != "Linux" or platform.machine() != "x86_64":
        raise ValueError("managed analysis requires Linux x86_64")
    catalog = json.loads((root / "tooling/configs/qa/tool-versions.json").read_text())
    item = catalog["externalTools"][name]
    version, asset = item["version"], item["linuxX64"]
    if (
        re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", version) is None
        or re.fullmatch(r"[0-9a-f]{64}", asset["sha256"]) is None
    ):
        raise ValueError("invalid analysis version or checksum")
    allowed = {
        "jscpd": f"https://github.com/kucherenko/jscpd/releases/download/v{version}/jscpd-linux-x64-gnu.tar.gz",
        "codeql": f"https://github.com/github/codeql-action/releases/download/codeql-bundle-v{version}/codeql-bundle-linux64.tar.zst",
    }
    if name not in allowed or asset["url"] != allowed[name]:
        raise ValueError("unapproved analysis archive URL")
    base = installation_base(name, root)
    destination = base / f"{name}-{version}"
    receipt = base / f"{name}-{version}.json"
    relative = "codeql/codeql" if name == "codeql" else "jscpd"
    if destination.exists() and receipt.exists():
        saved = json.loads(receipt.read_text())
        if saved != {"archiveSha256": asset["sha256"], "treeSha256": tree_digest(destination)}:
            raise ValueError("installed analysis tool identity changed")
        return destination / relative
    base.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=f"{name}-install-", dir=base) as directory:
        temporary = Path(directory)
        archive = temporary / asset["url"].rsplit("/", 1)[1]
        # A verified probe download can seed the managed installation without another transfer.
        seed = root / ".tmp/qa-tools/analysis-probe" / archive.name
        if seed.is_file() and sha256(seed) == asset["sha256"]:
            shutil.copyfile(seed, archive)
        else:
            request = urllib.request.Request(asset["url"], headers={"User-Agent": "SniptaleAnalysis/1"})
            with urllib.request.urlopen(request, timeout=60) as response, archive.open("wb") as output:
                if urllib.parse.urlparse(response.geturl()).hostname not in {
                    "github.com",
                    "release-assets.githubusercontent.com",
                    "objects.githubusercontent.com",
                }:
                    raise ValueError("unapproved analysis archive redirect")
                total = 0
                while chunk := response.read(1024 * 1024):
                    total += len(chunk)
                    if total > 1024**3:
                        raise ValueError("analysis archive download exceeds budget")
                    output.write(chunk)
        if sha256(archive) != asset["sha256"]:
            raise ValueError("analysis archive checksum mismatch")
        unpacked = temporary / "unpacked"
        unpacked.mkdir()
        extract_archive(archive, unpacked)
        executable = unpacked / relative
        args = [str(executable), "version", "--format=json"] if name == "codeql" else [str(executable), "--version"]
        result = subprocess.check_output(args, text=True, timeout=30)
        actual = json.loads(result)["version"] if name == "codeql" else result.strip().removeprefix("jscpd ")
        if actual != version:
            raise ValueError("installed analysis version differs from catalog")
        identity = tree_digest(unpacked)
        if destination.exists():
            raise ValueError("incomplete analysis installation requires explicit removal")
        unpacked.rename(destination)
        atomic_write_json(receipt, {"archiveSha256": asset["sha256"], "treeSha256": identity})
    return destination / relative


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("tools", nargs="*", choices=["jscpd", "codeql"])
    args = parser.parse_args()
    check()
    for name in args.tools or ["jscpd", "codeql"]:
        install(name)
        print(f"Installed verified {name}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
