#!/usr/bin/env bash
set -euo pipefail
export PYTHONDONTWRITEBYTECODE=1

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
# shellcheck source=../runtime/lib.sh
source "$ROOT/tooling/qa/runtime/lib.sh"
ensure_rust_env
cd "$ROOT"

require_command cargo
require_command python3
qa_init "qa:checkpoint" blocking
qa_require_harness
qa_step "fast source admission" tooling/qa/wrappers/fast.sh
qa_fail_if_any "fast source admission summary"

qa_step "target artifact budget" tooling/qa/wrappers/prune-target.sh
qa_step "checkpoint static proof" python3 -m tooling.qa.runtime.checkpoint_checks
qa_fail_if_any "checkpoint static proof summary"
qa_step "check windows-msvc release executable" \
  env SNIPTALE_WINDOWS_RESOURCES=check-skip \
  cargo check --release --target x86_64-pc-windows-msvc --bin sniptale-native-agent --all-features
qa_step "check windows-msvc test compilation" \
  env SNIPTALE_WINDOWS_RESOURCES=check-skip \
  cargo check --target x86_64-pc-windows-msvc --tests --all-features
qa_step "clippy" cargo clippy --all-targets --all-features -- -D warnings
if command -v cargo-nextest >/dev/null 2>&1; then
  qa_step "tests nextest" \
    env NEXTEST_EXPERIMENTAL_LIBTEST_JSON=1 \
    cargo nextest run --all-targets --all-features --no-tests pass \
    --message-format libtest-json-plus
else
  qa_step "tests cargo" cargo test --all-targets --all-features
fi

qa_step "checkpoint input stability" python3 -m tooling.qa.runtime.cli assert-inputs --record "$SNIPTALE_QA_RUN_RECORD"
qa_fail_if_any "checkpoint summary"
qa_write_state checkpoint
printf 'checkpoint OK: %s\n' "$SNIPTALE_QA_RUN_ID"
