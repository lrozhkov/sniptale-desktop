#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
source "$ROOT/tooling/qa/runtime/lib.sh"
ensure_rust_env
cd "$ROOT"
while read -r name version; do
  if ! command -v "$name" >/dev/null 2>&1 || [[ "$("$name" --version)" != *" $version"* ]]; then
    features=()
    if [[ "$name" == "cargo-about" ]]; then
      features=(--features cli)
    fi
    cargo install --locked "$name" --version "$version" "${features[@]}"
  fi
  if ! command -v "$name" >/dev/null 2>&1 || [[ "$("$name" --version)" != *" $version"* ]]; then
    printf 'Pinned release tool is missing or has the wrong version: %s\n' "$name" >&2
    exit 1
  fi
done < <(python3 - <<'PY'
import json
with open('tooling/configs/qa/tool-versions.json') as stream:
    for name, version in json.load(stream)['cargoSubcommands'].items():
        print(name, version)
PY
)
rustup component add llvm-tools-preview
tooling/qa/installers/install-gitleaks.sh
