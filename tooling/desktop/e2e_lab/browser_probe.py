"""Create a deterministic ZIP of the browser Native Messaging probe."""

from __future__ import annotations

import os
import zipfile
from pathlib import Path


def build(root: Path, destination: Path, *, nonce: str = "0" * 64) -> Path:
    if len(nonce) != 64 or any(character not in "0123456789abcdef" for character in nonce):
        raise ValueError("browser probe nonce must be 64 lowercase hexadecimal characters")
    source = root / "tooling/desktop/e2e_lab/guest/browser-extension"
    destination.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    temporary = destination.with_suffix(".partial")
    with zipfile.ZipFile(temporary, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for path in sorted(source.iterdir()):
            info = zipfile.ZipInfo(path.name, date_time=(1980, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = 0o100644 << 16
            content = path.read_bytes()
            if path.name == "probe.js":
                content = content.replace(b"__SNIPTALE_E2E_NONCE__", nonce.encode("ascii"))
            archive.writestr(info, content, compress_type=zipfile.ZIP_DEFLATED, compresslevel=9)
    os.replace(temporary, destination)
    return destination
