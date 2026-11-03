#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$ROOT"

destination=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --destination)
      destination="${2:-}"
      shift 2
      ;;
    -h | --help)
      printf 'Usage: tooling/qa/installers/install-gitleaks.sh [--destination <file>]\n'
      printf 'Installs the pinned Linux x64 Gitleaks binary after SHA-256 verification.\n'
      exit 0
      ;;
    *)
      printf 'Unknown argument: %s\n' "$1" >&2
      exit 2
      ;;
  esac
done

if [[ "$(uname -s)" != "Linux" || "$(uname -m)" != "x86_64" ]]; then
  printf 'The managed Gitleaks installer supports Linux x86_64 only.\n' >&2
  exit 1
fi

mapfile -t metadata < <(
  python3 - <<'PY'
import json
from pathlib import Path

value = json.loads(Path("tooling/configs/qa/tool-versions.json").read_text(encoding="utf-8"))["externalTools"]["gitleaks"]
print(value["version"])
print(value["linuxX64"]["url"])
print(value["linuxX64"]["sha256"])
PY
)
version="${metadata[0]}"
url="${metadata[1]}"
expected_sha256="${metadata[2]}"
destination="${destination:-$ROOT/.tmp/qa-tools/gitleaks/$version/gitleaks}"

if [[ -x "$destination" && "$($destination version)" == "$version" ]]; then
  printf 'Gitleaks %s already installed: %s\n' "$version" "$destination"
  exit 0
fi

require_command() {
  command -v "$1" >/dev/null 2>&1 || {
    printf 'Missing required installer command: %s\n' "$1" >&2
    exit 1
  }
}
require_command curl
require_command sha256sum
require_command tar

mkdir -p "$ROOT/.tmp/qa-tools" "$(dirname "$destination")"
temporary="$(mktemp -d "$ROOT/.tmp/qa-tools/gitleaks-install.XXXXXX")"
cleanup() {
  rm -r -- "$temporary"
}
trap cleanup EXIT
archive="$temporary/gitleaks.tar.gz"
curl --fail --location --proto '=https' --tlsv1.2 --output "$archive" "$url"
printf '%s  %s\n' "$expected_sha256" "$archive" | sha256sum --check --status
tar -xzf "$archive" -C "$temporary" gitleaks
install -m 0755 "$temporary/gitleaks" "$destination"
if [[ "$($destination version)" != "$version" ]]; then
  printf 'Installed Gitleaks version does not match the pinned version.\n' >&2
  exit 1
fi
printf 'Installed Gitleaks %s: %s\n' "$version" "$destination"
