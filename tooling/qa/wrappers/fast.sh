#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
# shellcheck source=../runtime/lib.sh
source "$ROOT/tooling/qa/runtime/lib.sh"
# shellcheck source=../runtime/script-tools-env.sh
source "$ROOT/tooling/qa/runtime/script-tools-env.sh"
script_tools_env "$ROOT" --require-node
cd "$ROOT"
mode=source
case "${1:-}" in
  "") ;;
  --release) mode=release ;;
  *)
    printf 'Usage: fast.sh [--release]\n' >&2
    exit 2
    ;;
esac
export SNIPTALE_QA_FAST_MODE="$mode"
qa_init "qa:fast" blocking "$mode"
qa_step "parallel environment and source proof" python3 -m tooling.qa.runtime.fast_checks
qa_fail_if_any "fast proof summary"
