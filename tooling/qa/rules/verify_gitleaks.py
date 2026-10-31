#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path


REQUIRED_FINDING_FIELDS = {"RuleID", "File", "StartLine", "Fingerprint", "Secret"}


def baseline_matches(entry: dict, finding: dict, scope: str) -> bool:
    if entry.get("SniptaleEnabled") is not True:
        return False
    entry_scope = entry.get("SniptaleScope")
    if entry_scope not in {scope, "both"}:
        return False
    if any(entry.get(field) != finding.get(field) for field in ("RuleID", "File", "StartLine")):
        return False
    if entry_scope == "history":
        return entry.get("Fingerprint") == finding.get("Fingerprint")
    if scope == "worktree":
        return entry.get("Fingerprint") == finding.get("Fingerprint")
    commit = str(finding.get("Commit", ""))
    return finding.get("Fingerprint") == f"{commit}:{entry.get('Fingerprint', '')}"


def load_array(path: Path, label: str) -> list[dict]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise ValueError(f"cannot read {label}: {error}") from error
    if not isinstance(value, list) or any(not isinstance(item, dict) for item in value):
        raise ValueError(f"{label} must be a JSON array of objects")
    return value


def validate(report: list[dict], baseline: list[dict], scope: str) -> list[str]:
    errors: list[str] = []
    observed: dict[str, dict] = {}
    for finding in report:
        missing = REQUIRED_FINDING_FIELDS - set(finding)
        if missing:
            errors.append("Gitleaks finding is missing fields: " + ", ".join(sorted(missing)))
            continue
        fingerprint = finding["Fingerprint"]
        if not isinstance(fingerprint, str) or not fingerprint:
            errors.append("Gitleaks finding requires a non-empty fingerprint")
            continue
        if fingerprint in observed:
            errors.append(f"duplicate Gitleaks finding fingerprint: {fingerprint}")
        observed[fingerprint] = finding
        if finding["Secret"] != "REDACTED":
            errors.append(f"Gitleaks report is not fully redacted: {fingerprint}")
        if scope == "history" and not re.fullmatch(r"[0-9a-f]{40}", str(finding.get("Commit", ""))):
            errors.append(f"Gitleaks history finding lacks an exact commit: {fingerprint}")

    active = [
        entry
        for entry in baseline
        if entry.get("SniptaleEnabled") is True and entry.get("SniptaleScope") in {scope, "both"}
    ]
    matched: set[int] = set()
    for fingerprint, finding in observed.items():
        matches = [index for index, entry in enumerate(active) if baseline_matches(entry, finding, scope)]
        if not matches:
            errors.append(
                f"unreviewed Gitleaks finding: {finding.get('RuleID')} {finding.get('File')}:{finding.get('StartLine')} {fingerprint}"
            )
        elif len(matches) > 1:
            errors.append(f"ambiguous Gitleaks baseline match: {fingerprint}")
        else:
            matched.add(matches[0])
    for index, entry in enumerate(active):
        if index not in matched:
            errors.append(f"stale Gitleaks baseline fingerprint: {entry.get('Fingerprint')}")
    return errors


def sanitize(report: list[dict], scope: str) -> list[dict]:
    fields = ("RuleID", "File", "StartLine", "Commit", "Fingerprint")
    return [
        {
            **{field: finding.get(field) for field in fields},
            "Secret": "REDACTED",
            "SniptaleScope": scope,
        }
        for finding in report
    ]


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Validate a fully redacted Gitleaks report against exact reviewed fingerprints."
    )
    parser.add_argument("--scope", choices=("worktree", "history"), required=True)
    parser.add_argument("--report", type=Path, required=True)
    parser.add_argument("--baseline", type=Path, required=True)
    args = parser.parse_args()
    try:
        report = load_array(args.report, "Gitleaks report")
        errors = validate(report, load_array(args.baseline, "Gitleaks baseline"), args.scope)
        args.report.write_text(json.dumps(sanitize(report, args.scope), indent=2) + "\n", encoding="utf-8")
    except ValueError as error:
        errors = [str(error)]
    if errors:
        for error in errors:
            print(f"FAIL {error}", file=sys.stderr)
        return 1
    print(f"OK Gitleaks {args.scope} report")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
