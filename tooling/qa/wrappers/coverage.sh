#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
# shellcheck disable=SC1091
source "$ROOT/tooling/qa/runtime/lib.sh"
ensure_rust_env release
cd "$ROOT"

require_command cargo
require_command python3
qa_init "qa:coverage" blocking

if ! command -v cargo-llvm-cov >/dev/null 2>&1; then
  printf 'Missing cargo-llvm-cov. Install with: cargo install --locked cargo-llvm-cov\n' >&2
  exit 1
fi

min_lines="${COVERAGE_MIN_LINES:-$(python3 tooling/qa/rules/coverage-floor.py)}"
mkdir -p .tmp/qa/coverage
lcov_path=".tmp/qa/coverage/lcov.info"
summary_path=".tmp/qa/coverage/summary.txt"
metadata_path=".tmp/qa/coverage/metadata.json"
owner_floors_path=".tmp/qa/coverage/owner-floors.json"

write_coverage_summary() {
  cargo llvm-cov report --summary-only | tee "$summary_path"
}

qa_step "coverage llvm-cov" \
  cargo llvm-cov --all-features --workspace --lcov --output-path "$lcov_path" --fail-under-lines "$min_lines"
qa_step "coverage owner floors" python3 tooling/qa/rules/coverage-floor.py --check-lcov "$lcov_path"
qa_step "coverage summary artifact" write_coverage_summary
qa_fail_if_any "coverage summary"
fingerprint_worktree >.tmp/qa/coverage.stamp
python3 tooling/qa/rules/coverage-floor.py --owner-floors-json >"$owner_floors_path"
python3 - "$min_lines" "$summary_path" "$lcov_path" "$owner_floors_path" <<'PY' >"$metadata_path"
from __future__ import annotations

import json
import sys
from pathlib import Path

min_lines, summary_path, lcov_path, owner_floors_path = sys.argv[1:]
payload = {
    "schemaVersion": 1,
    "lineFloorPercent": int(min_lines),
    "summary": summary_path,
    "lcov": lcov_path,
    "ownerFloors": owner_floors_path,
    "stamp": ".tmp/qa/coverage.stamp",
}
print(json.dumps(payload, indent=2, sort_keys=True))
for artifact in (summary_path, lcov_path, owner_floors_path):
    if not Path(artifact).is_file():
        raise SystemExit(f"missing coverage artifact: {artifact}")
PY
printf 'coverage OK: line floor %s%%\n' "$min_lines"
