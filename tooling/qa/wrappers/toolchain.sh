#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
source "$ROOT/tooling/qa/runtime/lib.sh"
cd "$ROOT"
qa_init "qa:toolchain" blocking
qa_step "canonical toolchain" python3 tooling/qa/runtime/toolchain.py "$@"
qa_fail_if_any "toolchain summary"
