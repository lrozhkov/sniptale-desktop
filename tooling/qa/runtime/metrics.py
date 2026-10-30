"""Read-only timing reports from explicit canonical wrapper receipts."""

from __future__ import annotations

import argparse
import json
import re
from pathlib import Path

from .repository import ROOT
from .storage import read_json, sanitize_text


def load_run(root: Path, run_id: str) -> dict:
    if not re.fullmatch(r"[0-9]{8}T[0-9]{6}\.[0-9]{6}Z-[0-9a-f]{12}", run_id):
        raise ValueError("invalid QA run ID")
    paths = list((root / ".tmp/qa-observability/runs").glob(f"*/{run_id}.json"))
    if len(paths) != 1:
        raise ValueError("QA run ID must identify one retained receipt")
    record = read_json(paths[0])
    if (
        record.get("runId") != run_id
        or record.get("status") != "all-passed"
        or type(record.get("exitCode")) is not int
        or record["exitCode"] != 0
    ):
        raise ValueError("timing comparison requires a successful completed receipt")
    return record


def summarize(record: dict) -> dict:
    duration = record.get("durationMs")
    steps = record.get("steps")
    if type(duration) is not int or duration < 0 or not isinstance(steps, list) or len(steps) > 128:
        raise ValueError("invalid timing receipt")
    if any(
        not isinstance(step, dict)
        or type(step.get("durationMs")) is not int
        or step["durationMs"] < 0
        or type(step.get("exitCode")) is not int
        or step["exitCode"] != 0
        or not isinstance(step.get("label"), str)
        or len(step["label"]) > 256
        for step in steps
    ):
        raise ValueError("invalid or failed timing step")
    source = record.get("repository", {})
    if not isinstance(source, dict):
        raise ValueError("invalid timing source")
    for key, length in (("revision", 40), ("fingerprint", 64)):
        value = source.get(key)
        if value is not None and not (key == "revision" and value == "no-head"):
            if not isinstance(value, str) or not re.fullmatch(r"[0-9a-f]{" + str(length) + "}", value):
                raise ValueError("invalid timing source identity")
    if not isinstance(record.get("wrapperId"), str) or not re.fullmatch(r"qa:[a-z-]{1,64}", record["wrapperId"]):
        raise ValueError("invalid timing wrapper")
    # Child durations overlap the containing group: never add them to wall time.
    top = [step for step in steps if not step.get("parallelGroup")]
    children = [step for step in steps if step.get("parallelGroup")]
    return {
        "runId": record["runId"],
        "wrapperId": record["wrapperId"],
        "revision": source.get("revision"),
        "sourceFingerprint": source.get("fingerprint"),
        "wallMs": duration,
        "topLevelStepMs": sum(s["durationMs"] for s in top),
        "parallelWorkMs": sum(s["durationMs"] for s in children),
        "steps": [{"label": sanitize_text(s["label"], ROOT), "durationMs": s["durationMs"]} for s in top],
        "slowestChecks": [
            {"label": sanitize_text(s["label"], ROOT), "durationMs": s["durationMs"]}
            for s in sorted(children or top, key=lambda s: s["durationMs"], reverse=True)[:5]
        ],
    }


def compare(before: dict, after: dict) -> dict:
    if before["wrapperId"] != after["wrapperId"]:
        raise ValueError("compare the same wrapper")
    return {
        "before": before,
        "after": after,
        "savedMs": before["wallMs"] - after["wallMs"],
        "savedPercent": round(100 * (before["wallMs"] - after["wallMs"]) / before["wallMs"], 2)
        if before["wallMs"]
        else None,
        "note": "Compare like-for-like hardware, cache state and workloads; timings are not QA admission.",
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--run-id", required=True)
    parser.add_argument("--before-run-id")
    args = parser.parse_args()
    try:
        result = summarize(load_run(ROOT, args.run_id))
        if args.before_run_id:
            result = compare(summarize(load_run(ROOT, args.before_run_id)), result)
        print(json.dumps(result, indent=2))
        return 0
    except (OSError, ValueError, KeyError, TypeError):
        print("FAIL timing report: invalid or unavailable timing receipt")
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
