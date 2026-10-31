#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
# shellcheck source=../runtime/lib.sh
source "$ROOT/tooling/qa/runtime/lib.sh"
# shellcheck source=../runtime/script-tools-env.sh
source "$ROOT/tooling/qa/runtime/script-tools-env.sh"
script_tools_env "$ROOT" --require-node
cd "$ROOT"
qa_init "qa:script-lints" blocking
qa_step "canonical toolchain" python3 -m tooling.qa.runtime.toolchain --check
qa_step "installed checker identities" python3 -m tooling.qa.runtime.install_script_tools --check-installed
qa_step "script policy" python3 -m tooling.qa.rules.verify_script_quality
qa_step "Renovate update policy" python3 -m tooling.qa.runtime.renovate_policy
qa_step "CPython update feed proof" node tooling/qa/runtime/renovate-feed-proof.cjs
qa_fail_if_any "lint policy admission"
if [[ $# == 0 ]] && python3 -m tooling.qa.runtime.lint_receipts --reuse >/dev/null; then
  qa_step "verified lint proof reuse" python3 -m tooling.qa.runtime.lint_receipts --reuse
  qa_fail_if_any "reused lint proof summary"
  exit 0
fi
qa_step "script lint lanes" python3 -m tooling.qa.runtime.script_lints "$@"
qa_fail_if_any "static lint admission"
if [[ $# == 0 ]]; then
  qa_step "checker negative proof" python3 -m tooling.qa.runtime.lint_negatives
  qa_step "script callback failure proof" node --unhandled-rejections=strict tooling/qa/runtime/promise-handling-proof.cjs
  qa_step "JSON rejection proof" .tmp/qa-tools/script-tools/python/bin/python -m tooling.qa.runtime.json_contracts --self-test
fi
qa_fail_if_any "script lint summary"
if [[ $# == 0 ]]; then
  qa_step "lint input stability" python3 -m tooling.qa.runtime.cli assert-inputs --record "$SNIPTALE_QA_RUN_RECORD"
  qa_step "write scoped lint evidence" python3 -m tooling.qa.runtime.lint_receipts --record-scope
  qa_fail_if_any "lint input stability summary"
  qa_write_state script-lints
fi
