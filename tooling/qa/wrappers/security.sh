#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
# shellcheck source=../runtime/lib.sh
source "$ROOT/tooling/qa/runtime/lib.sh"
ensure_rust_env
cd "$ROOT"

require_command python3
qa_init "qa:security" non-blocking

qa_step "security guardrails" python3 tooling/qa/rules/verify_security.py
qa_step "contract guardrails" python3 tooling/qa/rules/verify_contracts.py

qa_fail_if_any "security summary"
printf 'security OK\n'
