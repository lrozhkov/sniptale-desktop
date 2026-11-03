"""Reuse only complete lint proof for identical sources, with local/CI provenance."""

from __future__ import annotations

import argparse
import os
import re
from datetime import datetime
from pathlib import Path
from typing import Any

from .repository import ROOT, fingerprint, revision
from .script_lints import LABELS, inventory
from .state import STATE_VERSION, assert_run_inputs, state_path
from .storage import atomic_write_json, read_json

REQUIRED = set(LABELS) | {
    "canonical toolchain",
    "installed checker identities",
    "script policy",
    "script lint lanes",
    "checker negative proof",
    "script callback failure proof",
    "JSON rejection proof",
    "lint input stability",
    "write scoped lint evidence",
    "Renovate update policy",
    "CPython update feed proof",
}
CONTEXT_KEYS = ("GITHUB_REPOSITORY", "GITHUB_RUN_ID", "GITHUB_RUN_ATTEMPT", "GITHUB_SHA")


def context() -> dict[str, str]:
    return {key: os.environ.get(key, "") for key in CONTEXT_KEYS}


def validate(record: dict[str, Any], source: dict[str, Any]) -> None:
    if (
        record.get("wrapperId") != "qa:script-lints"
        or record.get("status") != "all-passed"
        or type(record.get("exitCode")) is not int
        or record["exitCode"] != 0
        or record.get("lintScope") != source
        or not isinstance(record.get("repository"), dict)
        or record["repository"].get("revision") != source["revision"]
    ):
        raise ValueError("lint proof producer or source does not match")
    steps = record.get("steps")
    if not isinstance(steps, list) or not steps or len(steps) > 64:
        raise ValueError("invalid lint proof steps")
    labels = []
    for step in steps:
        if (
            not isinstance(step, dict)
            or step.get("outcome") != "passed"
            or type(step.get("exitCode")) is not int
            or step["exitCode"] != 0
            or step.get("problemIds") != []
            or not isinstance(step.get("label"), str)
        ):
            raise ValueError("lint proof contains a failed or skipped check")
        labels.append(step["label"])
    if any(labels.count(label) != 1 for label in REQUIRED):
        raise ValueError("lint proof lacks the complete required check set")
    if not isinstance(record.get("finishedAt"), str):
        raise ValueError("lint proof completion time is missing")
    finished = datetime.fromisoformat(record["finishedAt"].replace("Z", "+00:00"))
    if finished.tzinfo is None:
        raise ValueError("lint proof completion time lacks a timezone")


def current_source() -> dict[str, Any]:
    targets = sorted(
        {name for group in inventory().values() for name in group}
        | {
            "Cargo.lock",
            ".gitignore",
            ".gitattributes",
            ".editorconfig",
            "tooling/configs/qa/script-python-requirements.txt",
            "tooling/configs/qa/script-python-build-requirements.txt",
        }
    )
    return {"revision": revision(), "files": targets, "fingerprint": fingerprint(targets)}


def producer() -> dict[str, Any]:
    state = read_json(state_path(ROOT, "script-lints"))
    if (
        state.get("contractVersion") != STATE_VERSION
        or state.get("name") != "script-lints"
        or state.get("status") != "passed"
    ):
        raise ValueError("invalid lint producer pointer")
    run_id = state["producerRunId"]
    if not isinstance(run_id, str) or re.fullmatch(r"[0-9]{8}T[0-9]{6}\.[0-9]{6}Z-[0-9a-f]{12}", run_id) is None:
        raise ValueError("invalid lint producer identity")
    day = run_id[:4] + "-" + run_id[4:6] + "-" + run_id[6:8]
    record = read_json(ROOT / ".tmp/qa-observability/runs" / day / (run_id + ".json"))
    validate(record, current_source())
    return record


def reuse() -> None:
    artifact = os.environ.get("SNIPTALE_QA_LINT_PROOF")
    if artifact:
        if os.environ.get("GITHUB_ACTIONS") != "true" or artifact != ".tmp/qa-artifacts/script-lint-proof.json":
            raise ValueError("CI lint artifact is only admitted through the canonical workflow")
        proof = read_json(ROOT / artifact)
        if (
            set(proof) != {"schemaVersion", "context", "record"}
            or proof["schemaVersion"] != 1
            or proof["context"] != context()
            or not all(context().values())
        ):
            raise ValueError("lint artifact is from another workflow run, attempt or commit")
        validate(proof["record"], current_source())
    else:
        producer()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--reuse", action="store_true")
    group.add_argument("--export", type=Path)
    group.add_argument("--record-scope", action="store_true")
    args = parser.parse_args()
    try:
        if args.record_scope:
            path = Path(os.environ["SNIPTALE_QA_RUN_RECORD"])
            assert_run_inputs(path)
            record = read_json(path)
            if record.get("wrapperId") != "qa:script-lints":
                raise ValueError("scope evidence requires the canonical lint producer")
            record["lintScope"] = current_source()
            atomic_write_json(path, record)
        elif args.export:
            record = producer()
            if (
                os.environ.get("GITHUB_ACTIONS") != "true"
                or not all(context().values())
                or context()["GITHUB_SHA"] != revision()
            ):
                raise ValueError("lint artifact export requires the current GitHub workflow identity")
            atomic_write_json(args.export, {"schemaVersion": 1, "context": context(), "record": record})
        else:
            reuse()
        print("OK verified complete lint proof for identical sources and required checks")
        return 0
    except (OSError, KeyError, TypeError, ValueError, RuntimeError):
        print("Lint proof is not reusable; run the complete source checks")
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
