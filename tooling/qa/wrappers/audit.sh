#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
# shellcheck source=../runtime/lib.sh
source "$ROOT/tooling/qa/runtime/lib.sh"
cd "$ROOT"

qa_init "qa:audit" blocking
audit_report_dir=".tmp/qa/audit/$SNIPTALE_QA_RUN_ID"
mkdir -p "$audit_report_dir"

expected_version="$(python3 -c 'import json; print(json.load(open("tooling/configs/qa/tool-versions.json", encoding="utf-8"))["externalTools"]["gitleaks"]["version"])')"
managed_bin="$ROOT/.tmp/qa-tools/gitleaks/$expected_version/gitleaks"
if [[ -n "${SNIPTALE_GITLEAKS_BIN:-}" ]]; then
  gitleaks_bin="$SNIPTALE_GITLEAKS_BIN"
elif [[ -x "$managed_bin" ]]; then
  gitleaks_bin="$managed_bin"
elif command -v gitleaks >/dev/null 2>&1; then
  gitleaks_bin="$(command -v gitleaks)"
else
  gitleaks_bin=""
fi

validate_gitleaks() {
  if [[ -z "$gitleaks_bin" || ! -x "$gitleaks_bin" ]]; then
    printf 'Gitleaks %s is required. Run tooling/qa/installers/install-gitleaks.sh.\n' "$expected_version" >&2
    return 1
  fi
  local actual
  actual="$($gitleaks_bin version)"
  if [[ "$actual" != "$expected_version" ]]; then
    printf 'Gitleaks version mismatch: expected %s, got %s.\n' "$expected_version" "$actual" >&2
    return 1
  fi
}

validate_history() {
  if [[ "$(git rev-parse --is-shallow-repository)" != "false" ]]; then
    printf 'Gitleaks history scan requires a complete, non-shallow Git checkout.\n' >&2
    return 1
  fi
}

scan_scope() {
  local scope="$1"
  local report="$audit_report_dir/gitleaks-$scope.json"
  if [[ -z "$gitleaks_bin" || ! -x "$gitleaks_bin" ]]; then
    return 1
  fi
  if ! "$gitleaks_bin" "$scope" . \
    --config tooling/configs/qa/gitleaks.toml \
    --exit-code 0 \
    --log-level warn \
    --no-banner \
    --no-color \
    --redact=100 \
    --report-format json \
    --report-path "$report" \
    --timeout 300; then
    printf 'Gitleaks %s engine failed before report validation.\n' "$scope" >&2
    return 1
  fi
  local policy_scope="$scope"
  if [[ "$scope" == "dir" ]]; then
    policy_scope="worktree"
  elif [[ "$scope" == "git" ]]; then
    policy_scope="history"
  fi
  python3 tooling/qa/rules/verify_gitleaks.py \
    --scope "$policy_scope" \
    --report "$report" \
    --baseline tooling/configs/qa/gitleaks-baseline.json
}

qa_step "Gitleaks version" validate_gitleaks
qa_step "complete Git history" validate_history
qa_step "Gitleaks worktree" scan_scope dir
qa_step "Gitleaks history" scan_scope git
qa_fail_if_any "audit summary"
printf 'secret audit OK: %s\n' "$audit_report_dir"
