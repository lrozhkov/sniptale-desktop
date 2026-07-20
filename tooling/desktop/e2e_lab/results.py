"""Strict guest result parsing and JUnit output."""

from __future__ import annotations

import json
import re
import xml.etree.ElementTree as ET
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from errors import IntegrityError
from receipt import redact

VALID_STATUS = {"pass", "fail", "skip"}
CASE_NAME = re.compile(r"^[a-z0-9][a-z0-9._-]{0,127}$")


@dataclass(frozen=True)
class TestCase:
    name: str
    status: str
    duration_ms: int
    detail: str


@dataclass(frozen=True)
class GuestResult:
    lane: str
    cases: tuple[TestCase, ...]
    environment: dict[str, Any]
    artifacts: tuple[str, ...]

    @property
    def passed(self) -> bool:
        return bool(self.cases) and all(case.status == "pass" for case in self.cases)


def parse_guest_result(path: Path, *, expected_lane: str, required_cases: set[str]) -> GuestResult:
    try:
        if path.stat().st_size > 1024 * 1024:
            raise IntegrityError("guest result exceeds the 1 MiB schema limit")
    except OSError as exc:
        raise IntegrityError("guest result is missing or unreadable") from exc
    try:
        raw = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        raise IntegrityError("guest result is missing or invalid JSON") from exc
    if not isinstance(raw, dict) or raw.get("schema_version") != 1 or raw.get("lane") != expected_lane:
        raise IntegrityError("guest result identity does not match the requested lane")
    raw_cases = raw.get("tests")
    if not isinstance(raw_cases, list) or not raw_cases or len(raw_cases) > 256:
        raise IntegrityError("guest result contains no test cases")
    cases = []
    names = set()
    for item in raw_cases:
        if not isinstance(item, dict):
            raise IntegrityError("guest test case must be an object")
        name = item.get("name")
        status = item.get("status")
        duration = item.get("duration_ms")
        detail = item.get("detail", "")
        if not isinstance(name, str) or not CASE_NAME.fullmatch(name) or name in names:
            raise IntegrityError("guest test names must be unique non-empty strings")
        if status not in VALID_STATUS or not isinstance(duration, int) or duration < 0:
            raise IntegrityError(f"invalid guest test result: {name}")
        if not isinstance(detail, str) or len(detail) > 4096:
            raise IntegrityError(f"invalid guest test detail: {name}")
        names.add(name)
        cases.append(TestCase(name, status, duration, detail))
    missing = required_cases - names
    if missing:
        raise IntegrityError(f"guest silently omitted requested tests: {', '.join(sorted(missing))}")
    environment = raw.get("environment")
    artifacts = raw.get("artifacts", [])
    if (
        not isinstance(environment, dict)
        or len(json.dumps(environment)) > 64 * 1024
        or not isinstance(artifacts, list)
        or len(artifacts) > 256
        or not all(isinstance(item, str) and 0 < len(item) <= 255 and Path(item).name == item for item in artifacts)
    ):
        raise IntegrityError("guest result environment or artifacts have invalid shape")
    return GuestResult(expected_lane, tuple(cases), environment, tuple(artifacts))


def write_junit(path: Path, results: list[GuestResult]) -> None:
    tests = sum(len(result.cases) for result in results)
    failures = sum(case.status == "fail" for result in results for case in result.cases)
    skipped = sum(case.status == "skip" for result in results for case in result.cases)
    suite = ET.Element("testsuite", name="desktop-e2e-lab", tests=str(tests), failures=str(failures), skipped=str(skipped))
    for result in results:
        for case in result.cases:
            node = ET.SubElement(suite, "testcase", classname=result.lane, name=case.name, time=f"{case.duration_ms / 1000:.3f}")
            if case.status == "fail":
                ET.SubElement(node, "failure", message=_xml_safe(redact(case.detail or "guest assertion failed")))
            elif case.status == "skip":
                ET.SubElement(node, "skipped", message=_xml_safe(redact(case.detail or "guest skipped test")))
    ET.ElementTree(suite).write(path, encoding="utf-8", xml_declaration=True)


def _xml_safe(value: str) -> str:
    return "".join(character for character in value if character in "\t\n\r" or ord(character) >= 0x20)
