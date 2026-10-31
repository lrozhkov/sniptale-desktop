#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
# shellcheck source=../runtime/lib.sh
source "$ROOT/tooling/qa/runtime/lib.sh"
cd "$ROOT"
qa_init "qa:install-hooks" non-blocking

if ! git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  printf 'Cannot install hooks: not a git repository.\n' >&2
  exit 1
fi

mkdir -p .git/hooks
cp tooling/qa/hooks/pre-commit .git/hooks/pre-commit
chmod +x .git/hooks/pre-commit
printf 'Installed git pre-commit hook: .git/hooks/pre-commit\n'
