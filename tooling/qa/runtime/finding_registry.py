"""Exact tool-noise and temporary-debt records for observed analyzer findings."""

from __future__ import annotations

import hashlib
import json
import re
from datetime import date
from pathlib import Path
from typing import Any


FIELDS = {
    "id",
    "classification",
    "owner",
    "rationale",
    "risk",
    "reviewBy",
    "removalCondition",
    "targetAction",
    "source",
    "exactScope",
    "proofReferences",
}


def finding(tool: str, rule: str, locations: list[dict[str, Any]], message: str, blocking: bool) -> dict[str, Any]:
    normalized = sorted(locations, key=lambda item: (item["path"], item["snippet"]))
    identity = {"tool": tool, "rule": rule, "locations": normalized, "message": message}
    key = hashlib.sha256(json.dumps(identity, sort_keys=True).encode()).hexdigest()
    return {**identity, "fingerprint": key, "blocking": blocking}


def validate_locations(scope: dict, root: Path, label: str) -> list[str]:
    locations = scope.get("locations")
    if not isinstance(locations, list) or not locations:
        return [f"{label}: exact source locations required"]
    errors = []
    observed = set()
    for item in locations:
        if not isinstance(item, dict) or set(item) != {"path", "startLine", "endLine", "sourceSha256"}:
            errors.append(f"{label}: invalid source location")
            continue
        path = item["path"]
        if not isinstance(path, str) or path not in scope["exactPaths"]:
            errors.append(f"{label}: source location is outside exact paths")
            continue
        resolved = (root / path).resolve()
        if not resolved.is_relative_to(root.resolve()) or not resolved.is_file():
            errors.append(f"{label}: redirected or missing source location")
            continue
        start, end = item["startLine"], item["endLine"]
        lines = resolved.read_text().splitlines()
        if type(start) is not int or type(end) is not int or not 1 <= start <= end <= len(lines):
            errors.append(f"{label}: invalid source bounds")
            continue
        key = (path, start, end)
        if key in observed:
            errors.append(f"{label}: duplicate source location")
        observed.add(key)
        current = hashlib.sha256("\n".join(lines[start - 1 : end]).encode()).hexdigest()
        if item["sourceSha256"] != current:
            errors.append(f"{label}: stale source fingerprint")
    if {item[0] for item in observed} != set(scope["exactPaths"]):
        errors.append(f"{label}: source locations do not cover the exact scope")
    return errors


def validate(entries: Any, root: Path, today: date | None = None) -> list[str]:
    errors: list[str] = []
    if not isinstance(entries, list):
        return ["technicalDebt must be a list"]
    seen_ids, seen_sources = set(), set()
    for entry in entries:
        if not isinstance(entry, dict) or set(entry) != FIELDS:
            errors.append("finding registry requires exact metadata fields")
            continue
        if any(
            not isinstance(entry[key], str) or not entry[key].strip()
            for key in FIELDS - {"source", "exactScope", "proofReferences"}
        ):
            errors.append("finding registry requires non-empty scalar metadata")
            continue
        label = entry["id"]
        if label in seen_ids:
            errors.append(f"{label}: duplicate id")
        seen_ids.add(label)
        if entry["classification"] not in {"tool-noise", "temporary-debt"}:
            errors.append(f"{label}: invalid finding classification")
        try:
            if date.fromisoformat(entry["reviewBy"]) < (today or date.today()):
                errors.append(f"{label}: expired review date")
        except (TypeError, ValueError):
            errors.append(f"{label}: invalid review date")
        source = entry["source"]
        if not isinstance(source, dict) or set(source) != {"tool", "rule", "fingerprint"}:
            errors.append(f"{label}: invalid finding source")
            continue
        if any(not isinstance(value, str) or not value for value in source.values()):
            errors.append(f"{label}: invalid source values")
            continue
        key = (source["tool"], source["rule"], source["fingerprint"])
        if source["tool"] not in {"jscpd", "codeql"} or not isinstance(source["rule"], str) or not source["rule"]:
            errors.append(f"{label}: unsupported tool or rule")
        if not isinstance(source["fingerprint"], str) or re.fullmatch(r"[0-9a-f]{64}", source["fingerprint"]) is None:
            errors.append(f"{label}: invalid exact fingerprint")
        if key in seen_sources:
            errors.append(f"{label}: duplicate finding scope")
        seen_sources.add(key)
        scope = entry["exactScope"]
        paths = (
            scope.get("exactPaths") if isinstance(scope, dict) and set(scope) == {"exactPaths", "locations"} else None
        )
        if (
            not isinstance(paths, list)
            or not paths
            or any(not isinstance(path, str) for path in paths)
            or len(paths) != len(set(paths))
        ):
            errors.append(f"{label}: exact paths required")
            continue
        errors.extend(validate_locations(scope, root, label))
        if not isinstance(entry["proofReferences"], list) or not entry["proofReferences"]:
            errors.append(f"{label}: proof references required")
            continue
        for relative in [*paths, *entry["proofReferences"]]:
            if not isinstance(relative, str):
                errors.append(f"{label}: invalid reference")
                continue
            path = Path(relative)
            if path.is_absolute() or ".." in path.parts or any(char in relative for char in "*?["):
                errors.append(f"{label}: broad or external path")
            elif relative.startswith((".tmp/", ".artifacts/", ".git/")) or not (root / path).resolve().is_relative_to(
                root.resolve()
            ):
                errors.append(f"{label}: non-authoritative or redirected path")
            elif not (root / path).is_file():
                errors.append(f"{label}: stale path {relative}")
        if not entry["proofReferences"]:
            errors.append(f"{label}: proof references required")
    return errors


def classify(findings: list[dict[str, Any]], entries: list[dict[str, Any]]) -> tuple[list[dict], list[str]]:
    records = {(e["source"]["tool"], e["source"]["rule"], e["source"]["fingerprint"]): e for e in entries}
    observed = set()
    remaining = []
    errors = []
    for item in findings:
        key = (item["tool"], item["rule"], item["fingerprint"])
        record = records.get(key)
        if record is None:
            remaining.append(item)
            continue
        observed.add(key)
        if sorted(record["exactScope"]["exactPaths"]) != sorted({location["path"] for location in item["locations"]}):
            errors.append(f"{record['id']}: finding path mismatch")
        # Debt records never suppress blocking CodeQL findings.
        if item["blocking"] and record["classification"] == "temporary-debt":
            remaining.append(item)
        else:
            item["reviewedAs"] = record["classification"]
            item["registryId"] = record["id"]
    for key in records.keys() - observed:
        errors.append(f"{records[key]['id']}: registered finding is no longer present")
    return remaining, errors
