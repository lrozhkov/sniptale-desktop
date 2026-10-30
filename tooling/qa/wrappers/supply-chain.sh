#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
# shellcheck disable=SC1091
source "$ROOT/tooling/qa/runtime/lib.sh"
ensure_rust_env
cd "$ROOT"

require_command cargo
qa_init "qa:supply-chain" blocking
mkdir -p .tmp/release/supply-chain

if command -v cargo-audit >/dev/null 2>&1; then
  qa_step "cargo audit" cargo audit
  qa_step "cargo audit report" bash -c 'cargo audit --json > .tmp/release/supply-chain/cargo-audit.json'
else
  printf 'Missing cargo-audit. Install with: cargo install --locked cargo-audit\n' >&2
  exit 1
fi

if command -v cargo-deny >/dev/null 2>&1; then
  qa_step "cargo deny" cargo deny check advisories bans licenses sources
  qa_step "cargo deny report" bash -c 'cargo deny check advisories bans licenses sources > .tmp/release/supply-chain/cargo-deny.txt'
else
  printf 'Missing cargo-deny. Install with: cargo install --locked cargo-deny\n' >&2
  exit 1
fi

qa_step "dependency license bundle" tooling/qa/wrappers/licenses.sh --check

qa_fail_if_any "supply-chain summary"
mkdir -p .tmp/qa
dependency_fingerprint >.tmp/qa/supply-chain.stamp
printf 'supply-chain OK\n'
printf 'supply-chain reports: %s\n' ".tmp/release/supply-chain"
