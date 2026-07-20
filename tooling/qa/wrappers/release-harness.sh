#!/usr/bin/env bash
set -euo pipefail
export PYTHONDONTWRITEBYTECODE=1

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
# shellcheck disable=SC1091
source "$ROOT/tooling/qa/runtime/lib.sh"
cd "$ROOT"

qa_init "qa:release-harness" blocking
qa_step "documentation policy" python3 tooling/qa/rules/verify_docs.py
qa_step "tooling topology and registry" python3 tooling/qa/rules/verify_tooling.py
qa_step "Python harness tests" python3 -m unittest discover -s tooling/qa/tests -p 'test_*.py'
qa_step "desktop E2E controller tests" \
  python3 -m unittest discover -s tooling/desktop/e2e_lab/tests -p 'test_*.py'
qa_step "Bash wrapper syntax" bash -n \
  tooling/qa/wrappers/*.sh \
  tooling/qa/installers/*.sh \
  tooling/qa/runtime/lib.sh \
  tooling/desktop/e2e-lab \
  tooling/release/windows/smoke/*.sh \
  tooling/release/windows/package.sh
qa_step "architecture guardrail self-tests" python3 tooling/qa/rules/verify_architecture.py --self-test
qa_step "boundary guardrail self-tests" python3 tooling/qa/rules/verify_boundaries.py --self-test
qa_step "contract guardrail self-tests" python3 tooling/qa/rules/verify_contracts.py --self-test
qa_step "security guardrail self-tests" python3 tooling/qa/rules/verify_security.py --self-test
qa_fail_if_any "release harness summary"
qa_write_state release-harness
printf 'release harness OK: %s\n' "$SNIPTALE_QA_RUN_ID"
