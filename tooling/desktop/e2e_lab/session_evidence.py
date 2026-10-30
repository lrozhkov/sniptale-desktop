"""Integrity collection for diagnostic sessions; never release acceptance evidence."""

from __future__ import annotations

import re

from errors import IntegrityError
from state import read_json
from system import sha256_file

IDENTITY = re.compile(r"[0-9]{8}T[0-9]{6}Z(?:-[0-9]{3})?")


def inventory(directory):
    entries = []
    total = 0
    for path in sorted(directory.rglob("*")):
        if path.is_symlink():
            raise IntegrityError("development evidence contains symlink")
        if not path.is_file() or path.name == "development-receipt.json":
            continue
        total += path.stat().st_size
        if len(entries) >= 3000 or total > 1024 * 1024 * 1024:
            raise IntegrityError("development evidence exceeds collection budget")
        entries.append(
            {"name": path.relative_to(directory).as_posix(), "bytes": path.stat().st_size, "sha256": sha256_file(path)}
        )
    return entries


def collect(paths, session_id, iteration):
    if IDENTITY.fullmatch(session_id) is None or IDENTITY.fullmatch(iteration) is None:
        raise IntegrityError("invalid development evidence identity")
    directory = paths.scoped(paths.lab / "sessions" / session_id / "iterations" / iteration)
    receipt = read_json(directory / "development-receipt.json")
    if (
        receipt is None
        or receipt.get("mode") != "development-session"
        or receipt.get("release_admissible") is not False
        or receipt.get("session_id") != session_id
        or receipt.get("iteration") != iteration
    ):
        raise IntegrityError("invalid development evidence receipt")
    if receipt.get("result") not in {"pass", "fail"} or receipt.get("operation") not in {"build", "test"}:
        raise IntegrityError("invalid development evidence result or operation")
    timings = receipt.get("timings_ms")
    if (
        not isinstance(timings, dict)
        or not timings
        or any(
            not isinstance(key, str) or type(value) is not int or not 0 <= value <= 86400000
            for key, value in timings.items()
        )
    ):
        raise IntegrityError("invalid development phase timings")
    if not receipt.get("evidence") or receipt["evidence"] != inventory(directory):
        raise IntegrityError("development evidence inventory changed or missing")
    return {
        "verified": True,
        "release_admissible": False,
        "session_id": session_id,
        "iteration": iteration,
        "result": receipt["result"],
        "operation": receipt["operation"],
        "timings_ms": receipt["timings_ms"],
    }


def sanitize_real_extension(value):
    """Accept only the bounded, non-sensitive facts emitted by the real browser scenario."""

    def require(condition):
        if not condition:
            raise IntegrityError("invalid real extension evidence")

    def fields(item, names):
        require(isinstance(item, dict) and set(item) == set(names.split()))

    def automation(item):
        fields(item, "node playwrightCore")
        require(
            all(
                isinstance(version, str) and re.fullmatch(r"[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}", version)
                for version in item.values()
            )
        )

    require(isinstance(value, list) and len(value) == 2)
    seen = set()
    for item in value:
        fields(item, "browser profile_reused capture_trigger connection media reconnect resident_restart")
        require(
            isinstance(item["browser"], str) and item["browser"] in {"chrome", "edge"} and item["browser"] not in seen
        )
        seen.add(item["browser"])
        require(type(item["profile_reused"]) is bool and item["capture_trigger"] == "native-tray-current-screen")
        for name in ("connection", "reconnect", "resident_restart"):
            connection = item[name]
            fields(connection, "passed connectionState settingsApplied browserVersion automation")
            require(
                connection["passed"] is True
                and connection["connectionState"] == "connected"
                and connection["settingsApplied"] is True
            )
            versions = connection["browserVersion"]
            require(isinstance(versions, list) and 1 <= len(versions) <= 2)
            require(
                all(
                    isinstance(version, str) and re.fullmatch(r"(Chrome|Edg)/[0-9]{1,3}(\.[0-9]{1,5}){3}", version)
                    for version in versions
                )
            )
            require(any(version.startswith("Edg/" if item["browser"] == "edge" else "Chrome/") for version in versions))
            automation(connection["automation"])
        media = item["media"]
        fields(media, "passed sha256 bytes width height mimeType automation")
        require(media["passed"] is True and media["mimeType"] == "image/png")
        require(isinstance(media["sha256"], str) and re.fullmatch(r"[0-9a-f]{64}", media["sha256"]) is not None)
        require(type(media["bytes"]) is int and 0 < media["bytes"] <= 128 * 1024 * 1024)
        require(all(type(media[key]) is int and 0 < media[key] <= 16384 for key in ("width", "height")))
        automation(media["automation"])
    return value
