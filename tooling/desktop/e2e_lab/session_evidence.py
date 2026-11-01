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

    def histories(item, samples):
        """Bounds for this short physical-input fixture, not product history quotas."""
        fields(item, "cursor events")
        for kind, proof in item.items():
            fields(proof, "blocks records bytes sha256" + (" samples" if kind == "cursor" else ""))
            require(type(proof["blocks"]) is int and 1 <= proof["blocks"] <= 16)
            require(type(proof["records"]) is int and proof["blocks"] <= proof["records"] <= 512 * proof["blocks"])
            require(type(proof["bytes"]) is int and 0 < proof["bytes"] <= 1048576 - 4 * proof["blocks"])
            require(isinstance(proof["sha256"], str) and re.fullmatch(r"[0-9a-f]{64}", proof["sha256"]) is not None)
            if kind == "cursor":
                require(type(proof["samples"]) is int and 3 <= proof["samples"] <= proof["records"])
                require(proof["samples"] == samples)
            else:
                require(proof["records"] >= 6)
        require(item["cursor"]["sha256"] != item["events"]["sha256"])

    require(isinstance(value, list) and len(value) == 2)
    seen = set()
    for item in value:
        fields(
            item,
            "browser profile_reused capture_trigger connection selected media recovery recording reconnect resident_restart",
        )
        require(
            isinstance(item["browser"], str) and item["browser"] in {"chrome", "edge"} and item["browser"] not in seen
        )
        seen.add(item["browser"])
        require(type(item["profile_reused"]) is bool and item["capture_trigger"] == "native-tray-current-screen")
        for name in ("connection", "selected", "reconnect", "resident_restart"):
            connection = item[name]
            require(isinstance(connection, dict) and type(connection.get("selected")) is bool)
            selected = connection["selected"]
            names = "passed selected connectionState settingsApplied selectionStatus automation"
            fields(connection, names + (" browserVersion" if selected else ""))
            require(connection["passed"] is True)
            if not selected:
                require(name == "connection" and connection["settingsApplied"] is False)
                require(connection["connectionState"] in {"controlled-by-other-profile", "selection-required"})
                require(connection["selectionStatus"] in {"available", "selection-required"})
                automation(connection["automation"])
                continue
            require(connection["connectionState"] == "connected" and connection["settingsApplied"] is True)
            require(connection["selectionStatus"] == "selected")
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
        recovery = item["recovery"]
        fields(recovery, "cancelledRetained localPreserved explicitRecovery sourceReleased media")
        require(
            all(
                recovery[key] is True
                for key in ("cancelledRetained", "localPreserved", "explicitRecovery", "sourceReleased")
            )
        )
        for media in (item["media"], recovery["media"]):
            fields(media, "passed sha256 bytes width height mimeType storageKind automation")
            require(media["passed"] is True and media["mimeType"] == "image/png")
            require(isinstance(media["storageKind"], str) and media["storageKind"] in {"opfs", "indexeddb-blob"})
            require(isinstance(media["sha256"], str) and re.fullmatch(r"[0-9a-f]{64}", media["sha256"]) is not None)
            require(type(media["bytes"]) is int and 0 < media["bytes"] <= 128 * 1024 * 1024)
            require(all(type(media[key]) is int and 0 < media[key] <= 16384 for key in ("width", "height")))
            automation(media["automation"])
        recording = item["recording"]
        fields(recording, "media editor")
        video = recording["media"]
        immutable = isinstance(video, dict) and video.get("normalization") == "microseconds-q8-verified"
        fields(
            video,
            "passed sha256 bytes width height durationMs mimeType storageKind cursorSamples clickEvents normalization sourceCursorEmbedded automation"
            + (" historyAttachments" if immutable else ""),
        )
        require(video["passed"] is True and video["mimeType"] == "video/mp4" and video["storageKind"] == "opfs")
        require(isinstance(video["sha256"], str) and re.fullmatch(r"[0-9a-f]{64}", video["sha256"]) is not None)
        require(type(video["bytes"]) is int and 0 < video["bytes"] <= 64 * 1024 * 1024)
        require(all(type(video[key]) is int and 0 < video[key] <= 16384 for key in ("width", "height")))
        require(type(video["durationMs"]) is int and 0 < video["durationMs"] <= 60000)
        require(type(video["cursorSamples"]) is int and 3 <= video["cursorSamples"] <= (8192 if immutable else 4096))
        require(
            type(video["clickEvents"]) is int
            and (video["clickEvents"] == 3 if immutable else 3 <= video["clickEvents"] <= 2048)
        )
        require(video["sourceCursorEmbedded"] is False)
        if immutable:
            histories(video["historyAttachments"], video["cursorSamples"])
        else:
            require(video["normalization"] == "milliseconds-to-seconds-once")
        automation(video["automation"])
        editor = recording["editor"]
        fields(
            editor,
            "passed historySegments canvasWidth canvasHeight playbackAdvanced automation"
            + (" reopenedHistorySegments seekStart seekEnd reopened" if immutable else ""),
        )
        require(editor["passed"] is True and editor["playbackAdvanced"] is True)
        require(type(editor["historySegments"]) is int and 1 <= editor["historySegments"] <= 4096)
        require(all(type(editor[key]) is int and 0 < editor[key] <= 16384 for key in ("canvasWidth", "canvasHeight")))
        if immutable:
            require(all(editor[key] is True for key in ("seekStart", "seekEnd", "reopened")))
            require(type(editor["reopenedHistorySegments"]) is int and 1 <= editor["reopenedHistorySegments"] <= 4096)
        automation(editor["automation"])
    return value
