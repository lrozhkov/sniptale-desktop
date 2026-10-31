#!/usr/bin/env bash
set -euo pipefail

repo_root() {
  local root
  root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
  printf '%s\n' "$root"
}

qa_init() {
  umask 077
  local wrapper_id="${1:-qa:$(basename "$0" .sh)}"
  local blocking="${2:-non-blocking}"
  local root
  root="$(repo_root)"
  # Metadata bootstrap and diagnostics must work before tools are installed.
  if [[ "$wrapper_id" != qa:toolchain && "$wrapper_id" != qa:preflight && -f "$root/tooling/qa/runtime/script-tools-env.sh" ]]; then
    # shellcheck source=script-tools-env.sh
    source "$root/tooling/qa/runtime/script-tools-env.sh"
    script_tools_env "$root"
  fi
  local mode="${3:-default}"
  shift "$(($# < 3 ? $# : 3))" || true
  QA_SENSITIVE_VALUES=("$@")
  local command=(python3 -m tooling.qa.runtime.cli start --wrapper "$wrapper_id" --mode "$mode")
  if [[ "$blocking" == "blocking" ]]; then
    command+=(--blocking)
  fi
  local sensitive
  for sensitive in "$@"; do
    command+=(--sensitive "$sensitive")
  done
  local exports
  exports="$("${command[@]}")"
  eval "$exports"
  export SNIPTALE_QA_RUN_ID SNIPTALE_QA_ROOT_RUN_ID SNIPTALE_QA_RUN_RECORD
  export SNIPTALE_QA_RUN_LOG SNIPTALE_QA_LOCK_TOKEN
  QA_FAILED_STEPS=()
  QA_FAILED_CODES=()
  QA_STEP_SUMMARY=()
  QA_RUN_FINALIZED=0
  trap 'qa_on_exit $?' EXIT
  trap 'exit 130' INT
  trap 'exit 143' TERM
}

qa_on_exit() {
  local status="$1"
  trap - EXIT INT TERM
  if [[ -n "${QA_ACTIVE_RAW_LOG:-}" ]]; then
    rm -f -- "$QA_ACTIVE_RAW_LOG"
    QA_ACTIVE_RAW_LOG=""
  fi
  if [[ "${QA_RUN_FINALIZED:-1}" == 0 && -n "${SNIPTALE_QA_RUN_RECORD:-}" ]]; then
    python3 -m tooling.qa.runtime.cli finish \
      --record "$SNIPTALE_QA_RUN_RECORD" \
      --log "$SNIPTALE_QA_RUN_LOG" \
      --exit-code "$status" || true
    QA_RUN_FINALIZED=1
  fi
  return "$status"
}

qa_step() {
  local name="$1"
  shift

  local step_id
  step_id="qa.step.$(printf '%s' "$name" | tr '[:upper:] /:' '[:lower:]---' | tr -cd 'a-z0-9._-')"
  local start
  local status
  start="$(date +%s)"

  printf '%s\n' "-> $name"
  if declare -F "${1:-}" >/dev/null 2>&1; then
    local report_dir=".tmp/qa-reports/$SNIPTALE_QA_RUN_ID"
    mkdir -p "$report_dir"
    chmod 700 "$report_dir" 2>/dev/null || true
    QA_ACTIVE_RAW_LOG="$(mktemp "$report_dir/${step_id}.XXXXXX.raw")"
    chmod 600 "$QA_ACTIVE_RAW_LOG" 2>/dev/null || true

    local begin_command=(python3 -m tooling.qa.runtime.cli begin-step
      --log "$SNIPTALE_QA_RUN_LOG"
      --step-id "$step_id"
      --label "$name")
    local sensitive
    for sensitive in "${QA_SENSITIVE_VALUES[@]:-}"; do
      begin_command+=(--sensitive "$sensitive")
    done
    local started_data
    started_data="$("${begin_command[@]}")"

    if "$@" >"$QA_ACTIVE_RAW_LOG" 2>&1; then
      status=0
    else
      status=$?
    fi

    local started_at="${started_data%%|*}"
    local started_ns="${started_data##*|}"
    local finish_command=(python3 -m tooling.qa.runtime.cli finish-step
      --record "$SNIPTALE_QA_RUN_RECORD"
      --log "$SNIPTALE_QA_RUN_LOG"
      --step-id "$step_id"
      --label "$name"
      --started-at "$started_at"
      --started-ns "$started_ns"
      --exit-code "$status"
      --output "$QA_ACTIVE_RAW_LOG")
    for sensitive in "${QA_SENSITIVE_VALUES[@]:-}"; do
      finish_command+=(--sensitive "$sensitive")
    done
    if ! "${finish_command[@]}"; then
      status=1
    fi
    rm -f -- "$QA_ACTIVE_RAW_LOG"
    QA_ACTIVE_RAW_LOG=""

    local elapsed
    elapsed="$(($(date +%s) - start))"
    if [[ "$status" == 0 ]]; then
      printf 'OK %s (%ss)\n' "$name" "$elapsed"
      QA_STEP_SUMMARY+=("OK $name (${elapsed}s)")
    else
      printf 'FAIL %s (%ss) log=%s\n' "$name" "$elapsed" "$SNIPTALE_QA_RUN_LOG_RELATIVE" >&2
      tail -80 "$SNIPTALE_QA_RUN_LOG" >&2 || true
      QA_STEP_SUMMARY+=("FAIL $name (${elapsed}s)")
      QA_FAILED_STEPS+=("$name")
      QA_FAILED_CODES+=("$status")
    fi
    return 0
  fi

  local runtime_command=(python3 -m tooling.qa.runtime.cli step
    --record "$SNIPTALE_QA_RUN_RECORD"
    --log "$SNIPTALE_QA_RUN_LOG"
    --step-id "$step_id"
    --label "$name")
  local sensitive
  for sensitive in "${QA_SENSITIVE_VALUES[@]:-}"; do
    runtime_command+=(--sensitive "$sensitive")
  done
  if [[ "${QA_LIVE_OUTPUT:-0}" == 1 ]]; then
    runtime_command+=(--stream-output)
  fi
  runtime_command+=(-- "$@")
  if "${runtime_command[@]}"; then
    local elapsed
    elapsed="$(($(date +%s) - start))"
    printf 'OK %s (%ss)\n' "$name" "$elapsed"
    QA_STEP_SUMMARY+=("OK $name (${elapsed}s)")
    return 0
  else
    status=$?
    local elapsed
    elapsed="$(($(date +%s) - start))"
    printf 'FAIL %s (%ss) log=%s\n' "$name" "$elapsed" "$SNIPTALE_QA_RUN_LOG_RELATIVE" >&2
    tail -80 "$SNIPTALE_QA_RUN_LOG" >&2 || true
    QA_STEP_SUMMARY+=("FAIL $name (${elapsed}s)")
    QA_FAILED_STEPS+=("$name")
    QA_FAILED_CODES+=("$status")
    return 0
  fi
}

qa_print_summary() {
  local title="${1:-QA summary}"
  printf '\n== %s ==\n' "$title"
  if [[ ${#QA_STEP_SUMMARY[@]} -eq 0 ]]; then
    printf 'No steps recorded.\n'
    return 0
  fi
  printf '%s\n' "${QA_STEP_SUMMARY[@]}"
}

qa_fail_if_any() {
  local title="${1:-QA summary}"
  qa_print_summary "$title"
  if [[ ${#QA_FAILED_STEPS[@]} -gt 0 ]]; then
    printf 'Failed step(s): %s\n' "${QA_FAILED_STEPS[*]}" >&2
    exit "${QA_FAILED_CODES[0]}"
  fi
}

ensure_rust_env() {
  if [[ -f "$HOME/.cargo/env" ]]; then
    # Rustup owns this user-environment fragment; it is outside the repository script inventory.
    # shellcheck source=/dev/null
    source "$HOME/.cargo/env"
  fi
  # Local edit loops reuse each profile's own incremental cache. Release proof and CI keep the
  # prior non-incremental default; explicit caller configuration still takes precedence.
  local incremental_default=1
  if [[ "${1:-dev}" == release || "${CI:-}" == true || "${CI:-}" == 1 || "${GITHUB_ACTIONS:-}" == true ]]; then
    incremental_default=0
  fi
  export CARGO_INCREMENTAL="${CARGO_INCREMENTAL:-$incremental_default}"
}

fingerprint_worktree() {
  python3 -m tooling.qa.runtime.cli fingerprint
}

qa_write_state() {
  python3 -m tooling.qa.runtime.cli state write "$1" --run-id "$SNIPTALE_QA_RUN_ID"
}

qa_assert_state() {
  python3 -m tooling.qa.runtime.cli state assert "$1"
}

qa_require_harness() {
  python3 -m tooling.qa.runtime.cli require-harness
}

dependency_files_changed() {
  if ! git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    return 1
  fi
  [[ -n "$(git status --short -- Cargo.toml Cargo.lock deny.toml rust-toolchain.toml LICENSE NOTICE THIRD_PARTY_DEPENDENCIES.json THIRD_PARTY_NOTICES.md LICENSES tooling/configs/qa/about.toml tooling/configs/qa/open-source.json tooling/configs/qa/tool-versions.json tooling/configs/qa/toolchain-lock.json tooling/release/legal)" ]]
}

high_risk_files_changed() {
  if ! git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    return 1
  fi
  [[ -n "$(git status --short -- \
    contracts/native \
    contracts/security \
    src/protocol \
    src/ipc \
    src/transfer \
    src/settings \
    src/assets \
    src/diagnostics \
    src/install \
    src/recording \
    src/capture \
    src/tray)" ]]
}

dependency_fingerprint() {
  {
    for path in Cargo.toml Cargo.lock deny.toml rust-toolchain.toml LICENSE NOTICE THIRD_PARTY_DEPENDENCIES.json THIRD_PARTY_NOTICES.md tooling/configs/qa/about.toml tooling/configs/qa/open-source.json tooling/configs/qa/tool-versions.json tooling/configs/qa/toolchain-lock.json tooling/release/legal/third-party-notices.hbs; do
      if [[ -f "$path" ]]; then
        sha256sum "$path"
      fi
    done
    if [[ -d LICENSES/third-party ]]; then
      find LICENSES/third-party -type f -print0 | sort -z | xargs -0 -r sha256sum
    fi
  } | sha256sum | awk '{print $1}'
}

require_command() {
  local name="$1"
  if ! command -v "$name" >/dev/null 2>&1; then
    printf 'Missing required command: %s\n' "$name" >&2
    return 1
  fi
}
