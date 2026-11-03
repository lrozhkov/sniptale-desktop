"""The advisory audit and source linters run independently before heavy work."""

from __future__ import annotations

import os
from pathlib import Path

from .parallel import run_group

CHECKS = [
    ("script quality proof", ["tooling/qa/wrappers/script-lints.sh"]),
]


def main() -> int:
    checks = list(CHECKS)
    if os.environ.get("SNIPTALE_QA_FAST_MODE") == "release":
        checks.append(("environment advisory audit", ["tooling/qa/wrappers/environment-audit.sh"]))
    return run_group(checks, Path(os.environ["SNIPTALE_QA_RUN_RECORD"]), Path(os.environ["SNIPTALE_QA_RUN_LOG"]), 2)


if __name__ == "__main__":
    raise SystemExit(main())
