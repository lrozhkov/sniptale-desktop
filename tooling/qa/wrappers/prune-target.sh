#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
# shellcheck source=../runtime/lib.sh
source "$ROOT/tooling/qa/runtime/lib.sh"
cd "$ROOT"
qa_init "qa:prune-target" non-blocking

dry_run=0
case "${1:-}" in
  "")
    ;;
  --dry-run)
    dry_run=1
    ;;
  -h | --help)
    printf '%s\n' \
      'Usage: tooling/qa/wrappers/prune-target.sh [--dry-run]' \
      '' \
      'Cleans the repository Cargo target directory when it exceeds the configured budget.' \
      'Environment:' \
      '  SNIPTALE_TARGET_MAX_GIB                budget in GiB (default: 12)' \
      '  SNIPTALE_TARGET_ALLOW_EXTERNAL_CLEAN   set to 1 to clean CARGO_TARGET_DIR outside the repo'
    exit 0
    ;;
  *)
    printf 'Unknown argument: %s\n' "$1" >&2
    exit 2
    ;;
esac

max_gib="${SNIPTALE_TARGET_MAX_GIB:-12}"
if [[ ! "$max_gib" =~ ^[1-9][0-9]*$ ]]; then
  printf 'SNIPTALE_TARGET_MAX_GIB must be a positive integer, got: %s\n' "$max_gib" >&2
  exit 2
fi

configured_target="${CARGO_TARGET_DIR:-$ROOT/target}"
if [[ "$configured_target" != /* ]]; then
  configured_target="$ROOT/$configured_target"
fi
target_dir="$(realpath -m -- "$configured_target")"
repo_target="$(realpath -m -- "$ROOT/target")"

if [[ "$target_dir" != "$repo_target" && "${SNIPTALE_TARGET_ALLOW_EXTERNAL_CLEAN:-0}" != "1" ]]; then
  printf 'target budget skipped: refusing to clean external CARGO_TARGET_DIR=%s\n' "$target_dir"
  exit 0
fi

if [[ ! -d "$target_dir" ]]; then
  printf 'target budget OK: %s does not exist\n' "$target_dir"
  exit 0
fi

size_kib="$(du -sk -- "$target_dir" | awk '{print $1}')"
limit_kib="$((max_gib * 1024 * 1024))"
size_gib="$(awk -v kib="$size_kib" 'BEGIN { printf "%.1f", kib / 1024 / 1024 }')"

if ((size_kib <= limit_kib)); then
  printf 'target budget OK: %s GiB used, limit %s GiB\n' "$size_gib" "$max_gib"
  exit 0
fi

if ((dry_run == 1)); then
  printf 'target budget exceeded: %s GiB used, limit %s GiB; cargo clean would run\n' \
    "$size_gib" "$max_gib"
  exit 0
fi

printf 'target budget exceeded: %s GiB used, limit %s GiB; running cargo clean\n' \
  "$size_gib" "$max_gib"
cargo clean --target-dir "$target_dir"
printf 'target cleanup OK: %s\n' "$target_dir"
