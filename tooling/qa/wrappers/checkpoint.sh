#!/usr/bin/env bash
set -euo pipefail
export PYTHONDONTWRITEBYTECODE=1

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
# shellcheck source=../runtime/lib.sh
source "$ROOT/tooling/qa/runtime/lib.sh"
# shellcheck source=../runtime/script-tools-env.sh
source "$ROOT/tooling/qa/runtime/script-tools-env.sh"
script_tools_env "$ROOT"
cd "$ROOT"

require_command python3
mode=scoped
case "${1:-}" in
  "") ;;
  --full) mode=full ;;
  *)
    printf 'Usage: checkpoint.sh [--full]\n' >&2
    exit 2
    ;;
esac
plan_args=()
if [[ "$mode" == full ]]; then plan_args+=(--full); fi
plan="$(python3 -m tooling.qa.runtime.cli plan "${plan_args[@]}")"
printf 'Checkpoint mode=%s checks=%s\n' "$mode" "$plan"
if [[ " $plan " == *" rust "* ]]; then
  ensure_rust_env
  require_command cargo
fi
if [[ " $plan " == *" ui "* || " $plan " == *" scripts "* ]]; then script_tools_env "$ROOT" --require-node; fi
export SNIPTALE_QA_CHECKPOINT_MODE="$mode"
qa_init "qa:checkpoint" blocking "$mode"
python3 -m tooling.qa.runtime.cli state clear checkpoint
python3 -m tooling.qa.runtime.cli state clear checkpoint-full
if [[ " $plan " == *" scripts "* || " $plan " == *" full "* ]]; then
  if ! qa_assert_state release-harness >/dev/null 2>&1; then
    qa_step "release harness" tooling/qa/wrappers/release-harness.sh
    qa_fail_if_any "checkpoint harness admission"
  fi
fi
if [[ " $plan " == *" scripts "* || " $plan " == *" ui "* ]]; then
  fast_args=()
  if [[ "$mode" == full || " $plan " == *" dependencies "* ]]; then fast_args+=(--release); fi
  qa_step "fast source admission" tooling/qa/wrappers/fast.sh "${fast_args[@]}"
  qa_fail_if_any "fast source admission summary"
fi

if [[ " $plan " == *" rust "* ]]; then qa_step "target artifact budget" tooling/qa/wrappers/prune-target.sh; fi
qa_step "checkpoint static proof" python3 -m tooling.qa.runtime.checkpoint_checks
qa_fail_if_any "checkpoint static proof summary"
if [[ " $plan " == *" rust "* ]]; then
  qa_step "check windows-msvc release executable" \
    env SNIPTALE_WINDOWS_RESOURCES=check-skip \
    cargo check --release --target x86_64-pc-windows-msvc --bin sniptale-native-agent --all-features
  qa_fail_if_any "Windows release compilation summary"
  qa_step "check windows-msvc test compilation" \
    env SNIPTALE_WINDOWS_RESOURCES=check-skip \
    cargo check --target x86_64-pc-windows-msvc --tests --all-features
  qa_fail_if_any "Windows test compilation summary"
  qa_step "clippy windows-msvc" \
    env SNIPTALE_WINDOWS_RESOURCES=check-skip \
    cargo clippy --target x86_64-pc-windows-msvc --all-targets --all-features -- -D warnings
  qa_step "clippy" cargo clippy --all-targets --all-features -- -D warnings
  qa_fail_if_any "compiler and lint summary"
  if command -v cargo-nextest >/dev/null 2>&1; then
    test_profile=dev
    if [[ " $plan " == *" long-tests "* ]]; then test_profile=default; fi
    qa_step "tests nextest" \
      env NEXTEST_EXPERIMENTAL_LIBTEST_JSON=1 \
      cargo nextest run --profile "$test_profile" --all-targets --all-features \
      --message-format libtest-json-plus
  else
    qa_step "tests cargo" cargo test --all-targets --all-features
  fi
fi

qa_step "checkpoint input stability" python3 -m tooling.qa.runtime.cli assert-inputs --record "$SNIPTALE_QA_RUN_RECORD"
qa_fail_if_any "checkpoint summary"
qa_write_state checkpoint
if [[ "$mode" == full ]]; then qa_write_state checkpoint-full; fi
printf 'checkpoint OK: %s\n' "$SNIPTALE_QA_RUN_ID"
