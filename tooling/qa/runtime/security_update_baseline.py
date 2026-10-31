"""Bind a new security age exception to the actual PR-base tool version."""

from __future__ import annotations

import os
import re
import subprocess
from pathlib import Path
from typing import Any

from .advisories import AdvisoryError, decode
from .repository import ROOT
from .environment_audit import catalog_tools
from .tool_dependencies import npm_packages


def git_object(commit: str, path: str) -> dict[str, Any] | None:
    def git(*arguments: str) -> bytes:
        return subprocess.run(["git", *arguments], cwd=ROOT, capture_output=True, check=True, timeout=20).stdout

    if git("cat-file", "-t", commit).strip() != b"commit":
        raise AdvisoryError("security update baseline is not a commit")
    entry = git("ls-tree", commit, "--", path).decode().strip()
    if not entry:
        return None
    metadata, name = entry.split("\t", 1)
    mode, kind, blob = metadata.split()
    if mode != "100644" or kind != "blob" or name != path or re.fullmatch(r"[0-9a-f]{40}", blob) is None:
        raise AdvisoryError("security update baseline input is redirected")
    if int(git("cat-file", "-s", blob)) > 8 * 1024 * 1024:
        raise AdvisoryError("security update baseline exceeds byte budget")
    return decode(git("cat-file", "blob", blob))


def versions() -> dict[str, str]:
    commit = "HEAD"
    if os.environ.get("GITHUB_EVENT_NAME") == "pull_request":
        with Path(os.environ["GITHUB_EVENT_PATH"]).open("rb") as stream:
            event = decode(stream.read(8 * 1024 * 1024 + 1))
        commit = event["pull_request"]["base"]["sha"]
        if not isinstance(commit, str) or re.fullmatch(r"[0-9a-f]{40}", commit) is None:
            raise AdvisoryError("invalid security update PR-base identity")
    value = git_object(commit, "tooling/configs/qa/tool-versions.json")
    if value is None:
        return {}
    result = catalog_tools(value)
    old_lock = git_object(commit, "tooling/configs/qa/script-node/package-lock.json")
    if old_lock is not None:
        grouped: dict[str, set[str]] = {}
        for _, name, version in npm_packages(old_lock):
            grouped.setdefault(name, set()).add(version)
        for name, selected in grouped.items():
            if name not in result and len(selected) == 1:
                result[name] = next(iter(selected))
    if any(not isinstance(version, str) for version in result.values()):
        raise AdvisoryError("security update baseline contains invalid versions")
    return result
