"""Catalog of independent checkpoint checks; Cargo compilation stays sequential."""

from __future__ import annotations

import os
from pathlib import Path

from .parallel import run_group


CHECKS = [
    ("fmt", ["cargo", "fmt", "--check"]),
    ("documentation policy", ["python3", "tooling/qa/rules/verify_docs.py"]),
    ("open-source legal policy", ["python3", "tooling/qa/rules/verify_legal.py"]),
    ("audit11 architecture snapshot", ["python3", "tooling/qa/rules/audit11_architecture_snapshot.py"]),
    ("tooling topology and exception registry", ["python3", "tooling/qa/rules/verify_tooling.py"]),
    ("architecture guardrail self-tests", ["python3", "tooling/qa/rules/verify_architecture.py", "--self-test"]),
    ("architecture guardrails", ["python3", "tooling/qa/rules/verify_architecture.py"]),
    ("topology guardrails", ["python3", "tooling/qa/rules/verify_topology.py"]),
    ("boundary guardrail self-tests", ["python3", "tooling/qa/rules/verify_boundaries.py", "--self-test"]),
    ("boundary guardrails", ["python3", "tooling/qa/rules/verify_boundaries.py"]),
    ("contract guardrail self-tests", ["python3", "tooling/qa/rules/verify_contracts.py", "--self-test"]),
    ("contract guardrails", ["python3", "tooling/qa/rules/verify_contracts.py"]),
    ("release artifact guardrails", ["python3", "tooling/release/windows/verify_release_artifacts.py"]),
    ("Windows resource guardrails", ["python3", "tooling/release/windows/verify_windows_resources.py"]),
    ("security guardrail self-tests", ["python3", "tooling/qa/rules/verify_security.py", "--self-test"]),
    ("security guardrails", ["python3", "tooling/qa/rules/verify_security.py"]),
    ("dev dependency guardrails", ["python3", "tooling/qa/rules/verify_dev_dependencies.py"]),
    ("test policy", ["python3", "tooling/qa/rules/verify_test_policy.py"]),
    (
        "desktop E2E lab controller tests",
        ["python3", "-m", "unittest", "discover", "-s", "tooling/desktop/e2e_lab/tests", "-p", "test_*.py"],
    ),
    ("QA runtime tests", ["python3", "-m", "unittest", "discover", "-s", "tooling/qa/tests", "-p", "test_*.py"]),
]


def main() -> int:
    try:
        workers = int(os.environ.get("SNIPTALE_QA_STATIC_JOBS", "4"))
        return run_group(
            CHECKS, Path(os.environ["SNIPTALE_QA_RUN_RECORD"]), Path(os.environ["SNIPTALE_QA_RUN_LOG"]), workers
        )
    except (KeyError, ValueError):
        print("FAIL checkpoint static proof: invalid worker configuration or missing wrapper context")
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
