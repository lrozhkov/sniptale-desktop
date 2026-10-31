"""Audit tracked Python/npm checking closures without installing or executing them."""

from __future__ import annotations

import re
from pathlib import Path
from typing import Any

from .advisories import AdvisoryError, Fetch, decode, osv_batch, request_json
from .repository import ROOT

REQUIREMENT = re.compile(r"([A-Za-z0-9_.-]+)==([0-9]+(?:\.[0-9]+){1,3}) --hash=sha256:[0-9a-f]{64}")


def npm_packages(lock: dict[str, Any]) -> list[tuple[str, str, str]]:
    if lock.get("lockfileVersion") != 3 or not isinstance(lock.get("packages"), dict):
        raise AdvisoryError("unsupported npm checking lock")
    result = []
    for path, item in lock["packages"].items():
        if not path:
            continue
        name = path.rsplit("node_modules/", 1)[-1]
        version = item.get("version")
        if (
            not path.startswith("node_modules/")
            or not re.fullmatch(r"(?:@[a-z0-9_.-]+/)?[a-z0-9_.-]+", name)
            or not isinstance(version, str)
            or re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+(?:-[A-Za-z0-9.-]+)?", version) is None
            or not item.get("resolved", "").startswith("https://registry.npmjs.org/")
            or re.fullmatch(r"sha512-[A-Za-z0-9+/]{86}==", item.get("integrity", "")) is None
        ):
            raise AdvisoryError("npm checking dependency lacks exact registry integrity")
        result.append(("npm", name, version))
    return result


def python_packages(source: str) -> list[tuple[str, str, str]]:
    result = []
    for line in source.splitlines():
        if not line or line.startswith("#"):
            continue
        match = REQUIREMENT.fullmatch(line)
        if match is None:
            raise AdvisoryError("Python checking dependency lacks exact version and hash")
        name = re.sub(r"[-_.]+", "-", match[1]).lower()
        result.append(("PyPI", name, match[2]))
    return result


def locked_packages(root: Path = ROOT) -> list[tuple[str, str, str]]:
    base = root / "tooling/configs/qa"
    packages = npm_packages(decode((base / "script-node/package-lock.json").read_bytes()))
    for name in ("script-python-requirements.txt", "script-python-build-requirements.txt"):
        packages.extend(python_packages((base / name).read_text()))
    return sorted(set(packages))


def audit(root: Path = ROOT, fetch: Fetch = request_json) -> list[dict[str, Any]]:
    selected = locked_packages(root)
    queries = [{"package": {"ecosystem": eco, "name": name}, "version": version} for eco, name, version in selected]
    findings = osv_batch(queries, fetch)
    return [
        {"ecosystem": item[0], "package": item[1], "version": item[2], "advisories": ids}
        for item, ids in zip(selected, findings, strict=True)
    ]
