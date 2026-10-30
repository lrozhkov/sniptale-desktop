#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
# shellcheck disable=SC1091
source "$ROOT/tooling/qa/runtime/lib.sh"
cd "$ROOT"

qa_init "qa:build" blocking

profile="dev"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --profile)
      profile="${2:-}"
      shift 2
      ;;
    --profile=*)
      profile="${1#*=}"
      shift
      ;;
    -h | --help)
      printf 'Usage: tooling/qa/wrappers/build.sh [--profile dev|release]\n'
      exit 0
      ;;
    *)
      printf 'Unknown argument: %s\n' "$1" >&2
      exit 2
      ;;
  esac
done

case "$profile" in
  dev | release)
    ;;
  *)
    printf 'Unknown build profile: %s\n' "$profile" >&2
    printf 'Usage: tooling/qa/wrappers/build.sh [--profile dev|release]\n' >&2
    exit 2
    ;;
esac

ensure_rust_env "$profile"
require_command cargo
qa_require_harness
qa_assert_state checkpoint

if [[ "$profile" == "release" ]]; then
  qa_step "build host release" cargo build --release --all-targets --all-features
  qa_step "check windows-msvc release target" env SNIPTALE_WINDOWS_RESOURCES=check-skip cargo check --release --target x86_64-pc-windows-msvc --all-targets --all-features
else
  qa_step "build host dev" cargo build --all-targets --all-features
  qa_step "check windows-msvc dev target" env SNIPTALE_WINDOWS_RESOURCES=check-skip cargo check --target x86_64-pc-windows-msvc --all-targets --all-features
fi

qa_fail_if_any "build summary"
qa_write_state build
printf 'build OK (%s)\n' "$profile"
