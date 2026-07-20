#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
# shellcheck disable=SC1091
source "$ROOT/tooling/qa/runtime/lib.sh"
ensure_rust_env
cd "$ROOT"

mode="check"
while [[ $# -gt 0 ]]; do
  case "$1" in
    --check)
      mode="check"
      shift
      ;;
    --write)
      mode="write"
      shift
      ;;
    -h|--help)
      printf 'Usage: tooling/qa/wrappers/licenses.sh [--check|--write]\n'
      printf 'Validates or explicitly regenerates tracked dependency legal outputs.\n'
      exit 0
      ;;
    *)
      printf 'Unknown argument: %s\n' "$1" >&2
      exit 2
      ;;
  esac
done

require_command cargo-about
qa_init "qa:licenses" blocking
candidate=".tmp/release/license-candidate/$SNIPTALE_QA_RUN_ID"
mkdir -p "$candidate" .tmp/release/supply-chain

qa_step "cargo-about JSON" cargo about generate --workspace --all-features --frozen --fail --format json --config tooling/configs/qa/about.toml --output-file "$candidate/cargo-about.json"
qa_step "cargo-about notices" cargo about generate --workspace --all-features --frozen --fail --config tooling/configs/qa/about.toml --output-file "$candidate/THIRD_PARTY_NOTICES.md" tooling/release/legal/third-party-notices.hbs
qa_step "dependency legal manifest" python3 tooling/release/legal/generate_dependency_licenses.py --cargo-about-json "$candidate/cargo-about.json" --output-dir "$candidate"

if [[ "$mode" == "write" ]]; then
  mkdir -p LICENSES/third-party
  find LICENSES/third-party -type f -delete
  cp "$candidate/THIRD_PARTY_DEPENDENCIES.json" THIRD_PARTY_DEPENDENCIES.json
  cp "$candidate/THIRD_PARTY_NOTICES.md" THIRD_PARTY_NOTICES.md
  cp "$candidate"/LICENSES/third-party/* LICENSES/third-party/
fi

install -m 0600 "$candidate/cargo-about.json" .tmp/release/supply-chain/cargo-about.json
qa_step "legal policy" python3 tooling/qa/rules/verify_legal.py --candidate "$candidate"
qa_fail_if_any "license summary"
printf 'licenses %s OK\n' "$mode"
