#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
# shellcheck disable=SC1091
source "$ROOT/tooling/qa/runtime/lib.sh"
ensure_rust_env
cd "$ROOT"

require_command python3
qa_init "qa:contract" non-blocking

qa_step "contract index" python3 tooling/qa/rules/verify_contracts.py
qa_step "test policy" python3 tooling/qa/rules/verify_test_policy.py

qa_fail_if_any "contract summary"
printf 'contract OK\n'
