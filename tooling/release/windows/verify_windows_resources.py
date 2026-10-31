#!/usr/bin/env python3
from __future__ import annotations

import struct
from pathlib import Path


ROOT = Path(__file__).resolve().parents[3]
RESOURCE = ROOT / "resources/windows"
REQUIRED_SIZES = {16, 20, 24, 32, 40, 48, 64, 128}


def ico_sizes(path: Path) -> set[int]:
    payload = path.read_bytes()
    if len(payload) < 6:
        raise ValueError("truncated ICO header")
    reserved, icon_type, count = struct.unpack_from("<HHH", payload)
    if reserved != 0 or icon_type != 1 or count == 0:
        raise ValueError("invalid ICO header")
    if len(payload) < 6 + (count * 16):
        raise ValueError("truncated ICO directory")
    sizes: set[int] = set()
    for index in range(count):
        width, height = struct.unpack_from("<BB", payload, 6 + (index * 16))
        if width != height:
            raise ValueError("non-square ICO entry")
        sizes.add(width or 256)
    return sizes


def main() -> int:
    errors: list[str] = []
    for name in ("sniptale.ico", "sniptale-recording.ico", "sniptale-paused.ico"):
        path = RESOURCE / "icons" / name
        try:
            sizes = ico_sizes(path)
        except (OSError, ValueError) as error:
            errors.append(f"{path.relative_to(ROOT)}: {error}")
            continue
        missing = sorted(REQUIRED_SIZES - sizes)
        if missing:
            errors.append(f"{path.relative_to(ROOT)} missing ICO sizes: {missing}")
    rc = (RESOURCE / "sniptale.rc").read_text(encoding="utf-8")
    for term in (
        "IDI_SNIPTALE_MAIN 101",
        "IDI_SNIPTALE_RECORDING 102",
        "IDI_SNIPTALE_PAUSED 103",
        "RT_MANIFEST 24",
    ):
        if term not in rc:
            errors.append(f"sniptale.rc missing {term}")
    manifest = (RESOURCE / "sniptale-native-agent.manifest").read_text(encoding="utf-8")
    for term in ("asInvoker", "PerMonitorV2", "longPathAware", "Common-Controls"):
        if term not in manifest:
            errors.append(f"manifest missing {term}")
    if errors:
        print("FAIL Windows resource guardrails", *[f"- {error}" for error in errors], sep="\n")
        return 1
    print("OK Windows resource guardrails")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
