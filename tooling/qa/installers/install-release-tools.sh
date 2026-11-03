#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
source "$ROOT/tooling/qa/runtime/lib.sh"
mode="${1:-all}"
case "$mode" in all | --nextest-only) ;; *) exit 2 ;; esac
export SNIPTALE_RELEASE_TOOLS_MODE="$mode"
ensure_rust_env
cd "$ROOT"
python3 tooling/qa/runtime/toolchain.py --check
while read -r name version; do
  if ! command -v "$name" >/dev/null 2>&1 || [[ "$(cargo "${name#cargo-}" --version)" != *" $version"* ]]; then
    features=()
    if [[ "$name" == "cargo-about" ]]; then
      features=(--features cli)
    fi
    cargo install --locked "$name" --version "$version" "${features[@]}"
  fi
  if ! command -v "$name" >/dev/null 2>&1 || [[ "$(cargo "${name#cargo-}" --version)" != *" $version"* ]]; then
    printf 'Pinned release tool is missing or has the wrong version: %s\n' "$name" >&2
    exit 1
  fi
done < <(
  python3 - <<'PY'
import json
import os
with open('tooling/configs/qa/tool-versions.json') as stream:
    for name, version in json.load(stream)['cargoSubcommands'].items():
        if os.environ['SNIPTALE_RELEASE_TOOLS_MODE'] != '--nextest-only' or name == 'cargo-nextest':
            print(name, version)
PY
)
if [[ "$mode" == --nextest-only ]]; then exit 0; fi
rustup component add llvm-tools-preview
tooling/qa/installers/install-gitleaks.sh

tooling/qa/installers/install-analysis-tools.sh
