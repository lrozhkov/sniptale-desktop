"""Extend the pinned upstream Framework profile with collected Windows 5.1 APIs."""

from __future__ import annotations

import json

from .advisories import decode, unique_object
from .repository import ROOT
from .storage import atomic_write_json


def prepare() -> None:
    config = ROOT / "tooling/configs/qa"
    version = decode((config / "tool-versions.json").read_bytes())["scriptTools"]["PSScriptAnalyzer"]["version"]
    baseline = (
        ROOT
        / ".tmp/qa-tools/script-tools"
        / ("PSScriptAnalyzer-" + version)
        / "compatibility_profiles/win-48_x64_10.0.17763.0_5.1.17763.316_x64_4.0.30319.42000_framework.json"
    )
    # The official profile contains a UTF-8 BOM and a type called "psobject";
    # PowerShell ConvertFrom-Json cannot safely round-trip it as a PSCustomObject.
    if baseline.stat().st_size > 64 * 1024 * 1024:
        raise ValueError("upstream compatibility profile exceeds byte budget")
    profile = json.loads(baseline.read_text(encoding="utf-8-sig"), object_pairs_hook=unique_object)
    addon = decode((config / "windows51-type-profile.json").read_bytes())
    assemblies = profile["Runtime"]["Types"]["Assemblies"]
    for name, entry in addon["types"].items():
        namespace, leaf = name.rsplit(".", 1)
        identity = entry["assembly"]
        assembly = assemblies.setdefault(identity["Name"], {"AssemblyName": identity, "Types": {}})
        if assembly["AssemblyName"] != identity:
            raise ValueError("Windows 5.1 type profile assembly identity mismatch")
        assembly["Types"].setdefault(namespace, {})[leaf] = entry["data"]
    profile["Id"] = "sniptale_windows51_framework"
    atomic_write_json(ROOT / ".tmp/qa-reports/windows51-profile.json", profile)


if __name__ == "__main__":
    prepare()
