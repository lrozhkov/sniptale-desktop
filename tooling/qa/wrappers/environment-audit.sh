#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
# shellcheck source=../runtime/lib.sh
source "$ROOT/tooling/qa/runtime/lib.sh"
source "$ROOT/tooling/qa/runtime/script-tools-env.sh"
script_tools_env "$ROOT"
cd "$ROOT"
qa_init "qa:environment-audit" blocking
qa_step "advisory-bound security update policy" python3 -m tooling.qa.runtime.security_updates
qa_step "build and QA tool advisories" python3 -m tooling.qa.runtime.environment_audit "$@"
qa_fail_if_any "environment audit summary"
