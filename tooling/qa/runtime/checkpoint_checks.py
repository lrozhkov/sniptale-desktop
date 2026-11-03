"""Catalog of independent checkpoint checks; Cargo compilation stays sequential."""

from __future__ import annotations

import os
from pathlib import Path

from .parallel import run_group
from .repository import check_plan


CHECKS = [
    ("fmt", ["cargo", "fmt", "--check"]),
    ("shared UI build", ["npm", "--prefix", "ui", "run", "build"]),
    ("shared UI contracts", ["npm", "--prefix", "ui", "test"]),
    ("shared UI browser behavior", ["npm", "--prefix", "ui", "run", "test:browser"]),
    ("documentation policy", ["python3", "tooling/qa/rules/verify_docs.py"]),
    ("open-source legal policy", ["python3", "tooling/qa/rules/verify_legal.py"]),
    ("audit11 architecture snapshot", ["python3", "tooling/qa/rules/audit11_architecture_snapshot.py"]),
    ("tooling topology and exception registry", ["python3", "tooling/qa/rules/verify_tooling.py"]),
    ("architecture guardrails", ["python3", "tooling/qa/rules/verify_architecture.py"]),
    ("topology guardrails", ["python3", "tooling/qa/rules/verify_topology.py"]),
    ("boundary guardrails", ["python3", "tooling/qa/rules/verify_boundaries.py"]),
    ("contract guardrails", ["python3", "tooling/qa/rules/verify_contracts.py"]),
    ("release artifact guardrails", ["python3", "tooling/release/windows/verify_release_artifacts.py"]),
    ("Windows resource guardrails", ["python3", "tooling/release/windows/verify_windows_resources.py"]),
    ("security guardrails", ["python3", "tooling/qa/rules/verify_security.py"]),
    ("dev dependency guardrails", ["python3", "tooling/qa/rules/verify_dev_dependencies.py"]),
    ("test policy", ["python3", "tooling/qa/rules/verify_test_policy.py"]),
]


def selected_checks(plan: list[str]) -> list[tuple[str, list[str]]]:
    # The harness owns its self-tests and Python/controller suites. Run them once there.
    return [
        (label, command)
        for label, command in CHECKS
        if (label != "fmt" or "rust" in plan) and (not label.startswith("shared UI ") or "ui" in plan)
    ]


def main() -> int:
    try:
        workers = int(os.environ.get("SNIPTALE_QA_STATIC_JOBS", "4"))
        return run_group(
            selected_checks(check_plan(full=os.environ.get("SNIPTALE_QA_CHECKPOINT_MODE") == "full")),
            Path(os.environ["SNIPTALE_QA_RUN_RECORD"]),
            Path(os.environ["SNIPTALE_QA_RUN_LOG"]),
            workers,
        )
    except (KeyError, ValueError):
        print("FAIL checkpoint static proof: invalid worker configuration or missing wrapper context")
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
