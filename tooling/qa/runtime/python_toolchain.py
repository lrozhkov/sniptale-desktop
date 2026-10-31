"""Resolve the official GitHub Actions Ubuntu Python distribution by exact version."""

from __future__ import annotations

import hashlib
import json
import re
from typing import Any


def validate(version: str, item: dict[str, Any]) -> None:
    tag = item.get("tag", "")
    if (
        re.fullmatch(re.escape(version) + r"-[0-9]+", tag) is None
        or item.get("source") != "https://api.github.com/repos/actions/python-versions/releases/tags/" + tag
        or item.get("url")
        != f"https://github.com/actions/python-versions/releases/download/{tag}/python-{version}-linux-24.04-x64.tar.gz"
        or re.fullmatch(r"[0-9a-f]{64}", item.get("sha256", "")) is None
    ):
        raise ValueError("Linux Python distribution identity is missing or stale")


def resolve(version: str, previous: dict[str, Any], fetch) -> dict[str, Any]:
    old = previous.get("linuxX64", {}) if previous.get("version") == version else {}
    if old:
        validate(version, old)
        return old
    releases = json.loads(
        fetch("https://api.github.com/repos/actions/python-versions/releases?per_page=100", 16 * 1024 * 1024)
    )
    release = next(
        (
            item
            for item in releases
            if item.get("name") == version and not item.get("prerelease") and not item.get("draft")
        ),
        None,
    )
    if release is None:
        raise ValueError("official Actions Python distribution for the selected version is unavailable")
    name = f"python-{version}-linux-24.04-x64.tar.gz"
    asset = next(item for item in release["assets"] if item["name"] == name)
    digest = asset.get("digest", "")
    item = {
        "tag": release["tag_name"],
        "publishedAt": release["published_at"],
        "source": "https://api.github.com/repos/actions/python-versions/releases/tags/" + release["tag_name"],
        "url": asset["browser_download_url"],
        "sha256": digest.removeprefix("sha256:") if digest.startswith("sha256:") else "0" * 64,
    }
    validate(version, item)
    if not digest.startswith("sha256:"):
        item["sha256"] = hashlib.sha256(fetch(item["url"], 128 * 1024 * 1024)).hexdigest()
    validate(version, item)
    return item
