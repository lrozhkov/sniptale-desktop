#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
source "$ROOT/tooling/qa/runtime/lib.sh"
ensure_rust_env release
cd "$ROOT"
qa_init "qa:alpha-proof" blocking
qa_step "release harness" tooling/qa/wrappers/release-harness.sh
qa_fail_if_any "alpha harness"
qa_step "canonical release proof" tooling/qa/wrappers/release.sh
qa_fail_if_any "alpha release proof"
qa_step "export commit-bound QA proof" python3 tooling/release/windows/alpha_artifacts.py proof "${1:-.tmp/release/alpha-proof}"
qa_fail_if_any "alpha proof"
