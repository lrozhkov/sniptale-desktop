#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
# shellcheck disable=SC1091
source "$ROOT/tooling/qa/runtime/lib.sh"
ensure_rust_env
cd "$ROOT"
qa_init "qa:preflight" non-blocking

printf '== Sniptale native preflight ==\n'
printf 'root: %s\n' "$ROOT"
printf 'date: %s\n' "$(date -Iseconds)"

printf '\n== git ==\n'
if git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  git status --short
else
  printf 'not a git repository\n'
fi

printf '\n== rust ==\n'
if command -v cargo >/dev/null 2>&1; then
  cargo --version
  rustc -Vv | sed -n '1,4p'
  rustup target list --installed 2>/dev/null || true
  rustup component list --installed 2>/dev/null || true
else
  printf 'cargo not found. Install Rust with rustup before implementation.\n'
fi

printf '\n== docs ==\n'
for path in \
  AGENTS.md \
  docs/architecture/repository-overview.md \
  docs/architecture/module-ownership.md \
  docs/engineering/implementation-rules.md \
  docs/security/threat-model.md \
  docs/tooling/qa.md \
  docs/oss/provenance.md \
  docs/engineering/testing-policy.md
do
  if [[ -f "$path" ]]; then
    printf 'OK %s\n' "$path"
  else
    printf 'MISSING %s\n' "$path"
  fi
done

printf '\n== architecture guardrails ==\n'
qa_step "architecture guardrails" python3 tooling/qa/rules/verify_architecture.py
qa_fail_if_any "preflight summary"
