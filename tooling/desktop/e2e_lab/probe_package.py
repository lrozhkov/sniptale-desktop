"""Build a byte-for-byte deterministic Linux probe deb without external libraries."""

from __future__ import annotations

import gzip
import io
import os
import stat
import tarfile
from pathlib import Path


def _tar_gz(files: dict[str, tuple[bytes, int]]) -> bytes:
    output = io.BytesIO()
    directories: set[str] = set()
    for name in files:
        parts = name.removeprefix("./").split("/")[:-1]
        for length in range(1, len(parts) + 1):
            directories.add("./" + "/".join(parts[:length]))
    with gzip.GzipFile(fileobj=output, mode="wb", mtime=0, filename="") as compressed:
        with tarfile.open(fileobj=compressed, mode="w", format=tarfile.GNU_FORMAT) as archive:
            for name in sorted(directories | set(files)):
                if name in directories:
                    info = tarfile.TarInfo(name)
                    info.type = tarfile.DIRTYPE
                    info.mode = 0o755
                    info.uid = 0
                    info.gid = 0
                    info.uname = "root"
                    info.gname = "root"
                    info.mtime = 0
                    archive.addfile(info)
                    continue
                data, mode = files[name]
                info = tarfile.TarInfo(name)
                info.size = len(data)
                info.mode = mode
                info.uid = 0
                info.gid = 0
                info.uname = "root"
                info.gname = "root"
                info.mtime = 0
                archive.addfile(info, io.BytesIO(data))
    return output.getvalue()


def _ar_member(name: str, data: bytes) -> bytes:
    mode = f"{0o100644:o}"
    header = f"{name + '/':<16}{0:<12}{0:<6}{0:<6}{mode:<8}{len(data):<10}`\n".encode("ascii")
    return header + data + (b"\n" if len(data) % 2 else b"")


def build(root: Path, destination: Path) -> Path:
    guest = root / "tooling/desktop/e2e_lab/guest"
    probe = (guest / "linux_probe.py").read_bytes()
    tray = (guest / "linux_tray.py").read_bytes()
    control = (
        "Package: sniptale-e2e-probe\n"
        "Version: 1.0.0\n"
        "Section: utils\n"
        "Priority: optional\n"
        "Architecture: all\n"
        "Maintainer: Sniptale QA\n"
        "Depends: python3, python3-gi, gir1.2-gtk-3.0, gir1.2-appindicator3-0.1\n"
        "Description: Deterministic Sniptale desktop E2E infrastructure probe\n"
    ).encode()
    control_tar = _tar_gz({"./control": (control, 0o644)})
    data_tar = _tar_gz({
        "./usr/bin/sniptale-e2e-probe": (probe, 0o755),
        "./usr/lib/sniptale-e2e-probe/tray.py": (tray, 0o755),
    })
    content = b"!<arch>\n" + _ar_member("debian-binary", b"2.0\n") + _ar_member("control.tar.gz", control_tar) + _ar_member("data.tar.gz", data_tar)
    destination.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    temporary = destination.with_suffix(".partial")
    temporary.write_bytes(content)
    temporary.chmod(stat.S_IRUSR | stat.S_IWUSR)
    os.replace(temporary, destination)
    return destination
