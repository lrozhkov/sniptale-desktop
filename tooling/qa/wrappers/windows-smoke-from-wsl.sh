#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
# shellcheck disable=SC1091
source "$ROOT/tooling/qa/runtime/lib.sh"
cd "$ROOT"

qa_init "qa:windows-smoke-from-wsl" blocking
qa_step "Windows smoke from WSL" tooling/release/windows/smoke/windows-smoke-from-wsl.sh "$@"
qa_fail_if_any "Windows smoke from WSL summary"
