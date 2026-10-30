#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
# shellcheck disable=SC1091
source "$ROOT/tooling/qa/runtime/lib.sh"
ensure_rust_env
cd "$ROOT"

require_command cargo
qa_init "qa:windows-smoke" blocking

if [[ "${1:-}" == "--alpha-artifact" ]]; then
  if [[ $# != 2 ]]; then
    printf 'Usage: tooling/qa/wrappers/windows-smoke.sh --alpha-artifact <bundle-msi>\n' >&2
    exit 2
  fi
  qa_step "exact alpha MSI in isolated Windows" tooling/desktop/e2e-lab test windows-11 --artifact "$2"
  qa_fail_if_any "alpha Windows smoke"
  exit 0
fi

ALLOWED_ORIGIN="${SNIPTALE_WINDOWS_SMOKE_EXTENSION_ORIGIN:-chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/}"
WRONG_ORIGIN="${SNIPTALE_WINDOWS_SMOKE_WRONG_ORIGIN:-chrome-extension://bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb/}"
EXE="$ROOT/target/debug/sniptale-native-agent.exe"

assert_contains() {
  local haystack="$1"
  local needle="$2"
  local label="$3"
  if [[ "$haystack" != *"$needle"* ]]; then
    printf '%s missing %s in output: %s\n' "$label" "$needle" "$haystack" >&2
    return 1
  fi
}

assert_support_mode() {
  local flag="$1"
  local mode="$2"
  local output
  output="$("$EXE" "$flag")"
  assert_contains "$output" "\"processMode\":\"$mode\"" "$flag"
}

assert_native_host_origin_gate() {
  local stdout stderr status
  stdout="$(mktemp)"
  stderr="$(mktemp)"
  if "$EXE" "$WRONG_ORIGIN" >"$stdout" 2>"$stderr"; then
    printf 'native-host wrong origin was accepted unexpectedly\n' >&2
    rm -f "$stdout" "$stderr"
    return 1
  fi
  if ! grep -q 'unsupported argument' "$stderr"; then
    printf 'native-host wrong origin did not fail as unsupported argument: %s\n' "$(cat "$stderr")" >&2
    rm -f "$stdout" "$stderr"
    return 1
  fi
  if [[ -s "$stdout" ]]; then
    printf 'native-host wrong origin wrote stdout unexpectedly: %s\n' "$(cat "$stdout")" >&2
    rm -f "$stdout" "$stderr"
    return 1
  fi
  rm -f "$stdout" "$stderr"

  stdout="$(mktemp)"
  stderr="$(mktemp)"
  status=0
  "$EXE" "$ALLOWED_ORIGIN" >"$stdout" 2>"$stderr" || status=$?
  if [[ "$status" -ne 2 ]]; then
    printf 'native-host allowed origin expected bridge-unavailable exit 2, got %s\n' "$status" >&2
    rm -f "$stdout" "$stderr"
    return 1
  fi
  if ! grep -q 'native host bridge unavailable' "$stderr"; then
    printf 'native-host allowed origin did not reach native-host mode: %s\n' "$(cat "$stderr")" >&2
    rm -f "$stdout" "$stderr"
    return 1
  fi
  rm -f "$stdout" "$stderr"
}

case "$(uname -s)" in
  MINGW* | MSYS* | CYGWIN*)
    ;;
  *)
    printf 'windows-smoke.sh requires a Windows host with MSVC Build Tools and Windows SDK.\n' >&2
    exit 2
    ;;
esac

qa_step "windows-msvc build" env \
  SNIPTALE_INSTALL_CHANNEL=stable \
  SNIPTALE_ALLOWED_EXTENSION_ORIGIN="$ALLOWED_ORIGIN" \
  cargo build --all-targets --all-features
qa_step "windows-msvc tests" cargo test --all-targets --all-features
qa_step "windows support mode probes" assert_support_mode "--version-json" "version-json"
qa_step "windows doctor mode probe" assert_support_mode "--doctor-json" "doctor-json"
qa_step "windows native-host origin gate" assert_native_host_origin_gate
qa_fail_if_any "windows smoke summary"
printf 'windows smoke OK\n'
