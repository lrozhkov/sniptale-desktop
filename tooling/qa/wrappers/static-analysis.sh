#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
mode="default"
if [[ "$#" == 2 && "$1" == "--review-run" && -n "$2" ]]; then
  mode="review-existing-results"
elif [[ "$#" != 0 ]]; then
  printf 'Usage: static-analysis.sh [--review-run RUN_ID]\n' >&2
  exit 2
fi
source "$ROOT/tooling/qa/runtime/lib.sh"
ensure_rust_env release
cd "$ROOT"
qa_init "qa:static-analysis" blocking "$mode"
qa_step "jscpd and CodeQL" python3 -m tooling.qa.runtime.static_analysis "$@"
qa_step "analysis input stability" python3 -m tooling.qa.runtime.cli assert-inputs --record "$SNIPTALE_QA_RUN_RECORD"
qa_fail_if_any "static analysis summary"
qa_write_state static-analysis
