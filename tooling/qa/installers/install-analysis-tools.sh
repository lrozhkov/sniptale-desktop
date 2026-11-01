#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
source "$ROOT/tooling/qa/runtime/lib.sh"
ensure_rust_env
# shellcheck source=../runtime/script-tools-env.sh
source "$ROOT/tooling/qa/runtime/script-tools-env.sh"
script_tools_env "$ROOT"
cd "$ROOT"
python3 -m tooling.qa.runtime.analysis_tools "$@"
