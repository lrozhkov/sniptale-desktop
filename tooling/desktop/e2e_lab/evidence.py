"""Strict verification for a completed desktop E2E evidence tree."""

from __future__ import annotations

import json
import os
import re
import stat
import xml.etree.ElementTree as ET
from decimal import Decimal, InvalidOperation
from pathlib import Path

from errors import IntegrityError
from receipt import validate_receipt
from system import sha256_file

SHA256 = re.compile(r"^[0-9a-f]{64}$")
COMMIT_ID = re.compile(r"^[0-9a-f]{40}(?:[0-9a-f]{24})?$")


def verify_run_evidence(run_dir: Path) -> dict[str, object]:
    receipt = _json(run_dir / "receipt.json")
    manifest = _json(run_dir / "artifact-manifest.json")
    evidence_manifest = _json(run_dir / "evidence-manifest.json")
    environment = _json(run_dir / "environment.json")
    validate_receipt(receipt)
    if receipt.get("run_id") != run_dir.name:
        raise IntegrityError("receipt run id does not match its evidence directory")
    if not COMMIT_ID.fullmatch(str(receipt.get("commit_sha", ""))) or not SHA256.fullmatch(str(receipt.get("dirty_state_sha256", ""))):
        raise IntegrityError("receipt source authority hashes are invalid")
    source = receipt.get("source")
    if not isinstance(source, dict) or not SHA256.fullmatch(str(source.get("sha256", ""))) or not isinstance(source.get("file_count"), int):
        raise IntegrityError("receipt source fingerprint is invalid")
    if receipt.get("artifact_manifest_sha256") != sha256_file(run_dir / "artifact-manifest.json"):
        raise IntegrityError("artifact manifest hash does not match receipt")
    if receipt.get("evidence_manifest_sha256") != sha256_file(run_dir / "evidence-manifest.json"):
        raise IntegrityError("evidence manifest hash does not match receipt")
    artifacts = _artifacts(manifest)
    if receipt.get("artifacts") != artifacts:
        raise IntegrityError("receipt artifact list differs from its manifest")
    if (
        environment.get("schema_version") != 1
        or environment.get("lanes") != receipt.get("snapshots")
        or set(environment.get("lanes", {})) != set(receipt.get("lanes", []))
    ):
        raise IntegrityError("environment and snapshot identities are incoherent")
    expected = _matrix(receipt)
    actual = _junit(run_dir / "junit.xml")
    if expected != actual:
        raise IntegrityError("JUnit results differ from the receipt test matrix")
    for lane in receipt["lanes"]:
        teardown = receipt.get("teardown", {}).get(lane)
        if not isinstance(teardown, dict) or not teardown or not all(isinstance(value, bool) for value in teardown.values()):
            raise IntegrityError(f"teardown evidence is missing or invalid for {lane}")
        if receipt["result"] == "pass" and any(teardown.values()):
            raise IntegrityError(f"passing receipt reports forced teardown for {lane}")
    files = _safe_evidence_tree(run_dir)
    declared_evidence = _evidence_files(evidence_manifest)
    actual_evidence = [
        {
            "relative_path": path.as_posix(),
            "category": path.parts[0],
            "bytes": (run_dir / path).stat().st_size,
            "sha256": sha256_file(run_dir / path),
        }
        for path in sorted(files, key=lambda value: value.as_posix())
    ]
    if declared_evidence != actual_evidence:
        raise IntegrityError("evidence files differ from the receipt-bound manifest")
    for directory in sorted(_required_evidence_directories(receipt, expected)):
        if not any(path.parts[-2] == directory for path in files if len(path.parts) >= 2):
            raise IntegrityError(f"test receipt has no {directory} evidence")
    if receipt["result"] == "pass" and any(status != "pass" for _lane, _name, status, _duration in expected):
        raise IntegrityError("passing receipt contains a failed or skipped requested test")
    return {
        "schema_version": 1,
        "run_id": receipt["run_id"],
        "result": receipt["result"],
        "tests": len(expected),
        "artifacts": len(artifacts),
        "evidence_files": len(files),
        "verified": True,
    }


def _required_evidence_directories(
    receipt: dict[str, object],
    expected: set[tuple[str, str, str, int]],
) -> set[str]:
    if not any(not name.startswith("build-") for _lane, name, _status, _duration in expected):
        return set()
    required = {"logs"}
    if receipt.get("run_mode", "full-acceptance") == "full-acceptance":
        return required | {"screenshots", "recordings"}
    if receipt.get("result") != "pass":
        return required
    requested = set(receipt.get("requested_cases", []))
    if requested & {"tray", "settings", "native-messaging", "capture"}:
        required.add("screenshots")
    if requested & {"recording-system-audio", "recording-microphone"}:
        required.add("recordings")
    return required


def _json(path: Path) -> dict[str, object]:
    try:
        descriptor = os.open(path, os.O_RDONLY | os.O_CLOEXEC | os.O_NOFOLLOW)
    except OSError as exc:
        raise IntegrityError(f"run evidence is missing or redirected: {path.name}") from exc
    try:
        metadata = os.fstat(descriptor)
        if not stat.S_ISREG(metadata.st_mode) or metadata.st_size > 4 * 1024 * 1024:
            raise IntegrityError(f"run evidence file type or size is invalid: {path.name}")
        with os.fdopen(descriptor, "r", encoding="utf-8") as stream:
            value = json.load(stream)
    except (OSError, json.JSONDecodeError) as exc:
        raise IntegrityError(f"run evidence JSON is invalid: {path.name}") from exc
    if not isinstance(value, dict):
        raise IntegrityError(f"run evidence JSON must be an object: {path.name}")
    return value


def _artifacts(manifest: dict[str, object]) -> list[dict[str, object]]:
    raw = manifest.get("artifacts")
    if manifest.get("schema_version") != 1 or not isinstance(raw, list) or len(raw) > 512:
        raise IntegrityError("artifact manifest schema is invalid")
    for item in raw:
        if (
            not isinstance(item, dict)
            or set(item) != {"name", "sha256", "bytes"}
            or not isinstance(item["name"], str)
            or Path(item["name"]).name != item["name"]
            or not SHA256.fullmatch(str(item["sha256"]))
            or not isinstance(item["bytes"], int)
            or item["bytes"] < 0
        ):
            raise IntegrityError("artifact manifest entry is invalid")
    return raw


def _evidence_files(manifest: dict[str, object]) -> list[dict[str, object]]:
    raw = manifest.get("files")
    if manifest.get("schema_version") != 1 or not isinstance(raw, list) or len(raw) > 4096:
        raise IntegrityError("evidence manifest schema is invalid")
    previous = ""
    for item in raw:
        if (
            not isinstance(item, dict)
            or set(item) != {"relative_path", "category", "bytes", "sha256"}
            or not isinstance(item["relative_path"], str)
            or not isinstance(item["category"], str)
            or item["category"] not in {"logs", "screenshots", "recordings"}
            or not isinstance(item["bytes"], int)
            or item["bytes"] < 0
            or not SHA256.fullmatch(str(item["sha256"]))
        ):
            raise IntegrityError("evidence manifest entry is invalid")
        relative = Path(item["relative_path"])
        if relative.is_absolute() or ".." in relative.parts or len(relative.parts) < 2 or relative.parts[0] != item["category"]:
            raise IntegrityError("evidence manifest path is invalid")
        if item["relative_path"] <= previous:
            raise IntegrityError("evidence manifest paths are not canonical and unique")
        previous = item["relative_path"]
    return raw


def _matrix(receipt: dict[str, object]) -> set[tuple[str, str, str, int]]:
    raw = receipt.get("test_matrix")
    if not isinstance(raw, list):
        raise IntegrityError("receipt test matrix is invalid")
    values: set[tuple[str, str, str, int]] = set()
    identities: set[tuple[str, str]] = set()
    matrix_lanes: set[str] = set()
    for lane in raw:
        if not isinstance(lane, dict) or not isinstance(lane.get("lane"), str) or not isinstance(lane.get("tests"), list):
            raise IntegrityError("receipt test matrix lane is invalid")
        if lane["lane"] in matrix_lanes:
            raise IntegrityError("receipt test matrix contains a duplicate lane")
        matrix_lanes.add(lane["lane"])
        if not lane["tests"]:
            raise IntegrityError("receipt test matrix contains an empty requested lane")
        for case in lane["tests"]:
            if (
                not isinstance(case, dict)
                or case.get("status") not in {"pass", "fail", "skip"}
                or not isinstance(case.get("name"), str)
                or not isinstance(case.get("duration_ms"), int)
                or isinstance(case.get("duration_ms"), bool)
                or case["duration_ms"] < 0
            ):
                raise IntegrityError("receipt test matrix case is invalid")
            identity = (lane["lane"], case["name"])
            if identity in identities:
                raise IntegrityError("receipt test matrix contains duplicate case identities")
            identities.add(identity)
            value = (lane["lane"], case["name"], case["status"], case["duration_ms"])
            values.add(value)
    if not values:
        raise IntegrityError("receipt test matrix is empty")
    if matrix_lanes != set(receipt.get("lanes", [])):
        raise IntegrityError("receipt test matrix silently omitted a requested lane")
    return values


def _junit(path: Path) -> set[tuple[str, str, str, int]]:
    try:
        root = ET.parse(path).getroot()
    except (OSError, ET.ParseError) as exc:
        raise IntegrityError("JUnit evidence is missing or invalid") from exc
    if root.tag != "testsuite":
        raise IntegrityError("JUnit root must be testsuite")
    values: set[tuple[str, str, str, int]] = set()
    identities: set[tuple[str, str]] = set()
    failures = 0
    skipped = 0
    for case in root.findall("testcase"):
        failure = case.find("failure") is not None
        skip = case.find("skipped") is not None
        if failure and skip:
            raise IntegrityError("JUnit case cannot be failed and skipped")
        status = "fail" if failure else "skip" if skip else "pass"
        failures += int(failure)
        skipped += int(skip)
        try:
            milliseconds = Decimal(case.get("time", "")) * 1000
        except InvalidOperation as exc:
            raise IntegrityError("JUnit testcase duration is invalid") from exc
        if milliseconds < 0 or milliseconds != milliseconds.to_integral_value():
            raise IntegrityError("JUnit testcase duration is invalid")
        identity = (case.get("classname", ""), case.get("name", ""))
        if identity in identities:
            raise IntegrityError("JUnit contains duplicate case identities")
        identities.add(identity)
        value = (*identity, status, int(milliseconds))
        values.add(value)
    if root.get("tests") != str(len(values)) or root.get("failures") != str(failures) or root.get("skipped") != str(skipped):
        raise IntegrityError("JUnit summary counters are incoherent")
    return values


def _safe_evidence_tree(run_dir: Path) -> list[Path]:
    files: list[Path] = []
    root = run_dir.resolve()
    for directory in ("logs", "screenshots", "recordings"):
        candidate = run_dir / directory
        if candidate.is_symlink() or not candidate.is_dir():
            raise IntegrityError(f"evidence directory is missing or redirected: {directory}")
        for path in candidate.rglob("*"):
            if path.is_symlink() or not path.resolve(strict=False).is_relative_to(root):
                raise IntegrityError("evidence tree contains a redirected path")
            if path.is_file():
                files.append(path.relative_to(run_dir))
            elif not path.is_dir():
                raise IntegrityError("evidence tree contains a special file")
    if len(files) > 4096:
        raise IntegrityError("evidence tree exceeds the file-count limit")
    return files
