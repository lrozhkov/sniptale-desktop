#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
# shellcheck disable=SC1091
source "$ROOT/tooling/qa/runtime/lib.sh"
ensure_rust_env
cd "$ROOT"
qa_init "qa:release" blocking

while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help)
      printf 'Usage: tooling/qa/wrappers/release.sh\n'
      printf 'Runs the full release QA gate without diff-based skips.\n'
      printf 'Use tooling/qa/wrappers/package-windows.sh on a Windows-capable builder for exe/MSI artifacts.\n'
      exit 0
      ;;
    *)
      printf 'Unknown argument: %s\n' "$1" >&2
      exit 2
      ;;
  esac
done

qa_require_harness
qa_step "secret audit" tooling/qa/wrappers/audit.sh
qa_step "checkpoint" tooling/qa/wrappers/checkpoint.sh
qa_step "coverage" tooling/qa/wrappers/coverage.sh
qa_step "supply chain" tooling/qa/wrappers/supply-chain.sh
qa_step "SBOM" python3 tooling/release/windows/generate-sbom.py --out .tmp/release/sbom.spdx.json
dependency_fingerprint > .tmp/release/sbom.dependency.stamp
qa_step "final checkpoint" tooling/qa/wrappers/checkpoint.sh
qa_step "release build" tooling/qa/wrappers/build.sh --profile release
qa_fail_if_any "release summary"

printf '\nrelease OK\n'
