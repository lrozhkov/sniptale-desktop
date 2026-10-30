#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
# shellcheck source=../runtime/lib.sh
source "$ROOT/tooling/qa/runtime/lib.sh"
# shellcheck source=../runtime/script-tools-env.sh
source "$ROOT/tooling/qa/runtime/script-tools-env.sh"
script_tools_env "$ROOT"
cd "$ROOT"
qa_init "qa:script-lints" blocking
qa_step "canonical toolchain" python3 -m tooling.qa.runtime.toolchain --check
qa_step "script lint lanes" python3 -m tooling.qa.runtime.script_lints "$@"
qa_fail_if_any "script lint summary"
