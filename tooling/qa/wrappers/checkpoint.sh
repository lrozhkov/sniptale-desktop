#!/usr/bin/env bash
set -euo pipefail
export PYTHONDONTWRITEBYTECODE=1

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
# shellcheck disable=SC1091
source "$ROOT/tooling/qa/runtime/lib.sh"
ensure_rust_env
cd "$ROOT"

require_command cargo
require_command python3
qa_init "qa:checkpoint" blocking
qa_require_harness

qa_step "target artifact budget" tooling/qa/wrappers/prune-target.sh
qa_step "fmt" cargo fmt --check
qa_step "documentation policy" python3 tooling/qa/rules/verify_docs.py
qa_step "open-source legal policy" python3 tooling/qa/rules/verify_legal.py
qa_step "audit11 architecture snapshot" python3 tooling/qa/rules/audit11_architecture_snapshot.py
qa_step "tooling topology and exception registry" python3 tooling/qa/rules/verify_tooling.py
qa_step "architecture guardrail self-tests" python3 tooling/qa/rules/verify_architecture.py --self-test
qa_step "architecture guardrails" python3 tooling/qa/rules/verify_architecture.py
qa_step "topology guardrails" python3 tooling/qa/rules/verify_topology.py
qa_step "boundary guardrail self-tests" python3 tooling/qa/rules/verify_boundaries.py --self-test
qa_step "boundary guardrails" python3 tooling/qa/rules/verify_boundaries.py
qa_step "contract guardrail self-tests" python3 tooling/qa/rules/verify_contracts.py --self-test
qa_step "contract guardrails" python3 tooling/qa/rules/verify_contracts.py
qa_step "release artifact guardrails" python3 tooling/release/windows/verify_release_artifacts.py
qa_step "Windows resource guardrails" python3 tooling/release/windows/verify_windows_resources.py
qa_step "security guardrail self-tests" python3 tooling/qa/rules/verify_security.py --self-test
qa_step "security guardrails" python3 tooling/qa/rules/verify_security.py
qa_step "dev dependency guardrails" python3 tooling/qa/rules/verify_dev_dependencies.py
qa_step "test policy" python3 tooling/qa/rules/verify_test_policy.py
qa_step "desktop E2E lab controller tests" \
  python3 -m unittest discover -s tooling/desktop/e2e_lab/tests -p 'test_*.py'
qa_step "QA runtime tests" python3 -m unittest discover -s tooling/qa/tests -p 'test_*.py'
qa_step "check windows-msvc release executable" \
  env SNIPTALE_WINDOWS_RESOURCES=check-skip \
  cargo check --release --target x86_64-pc-windows-msvc --bin sniptale-native-agent --all-features
qa_step "check windows-msvc test compilation" \
  env SNIPTALE_WINDOWS_RESOURCES=check-skip \
  cargo check --target x86_64-pc-windows-msvc --tests --all-features
qa_step "clippy" cargo clippy --all-targets --all-features -- -D warnings
if command -v cargo-nextest >/dev/null 2>&1; then
  qa_step "tests nextest" \
    env NEXTEST_EXPERIMENTAL_LIBTEST_JSON=1 \
      cargo nextest run --all-targets --all-features --no-tests pass \
      --message-format libtest-json-plus
else
  qa_step "tests cargo" cargo test --all-targets --all-features
fi

qa_fail_if_any "checkpoint summary"
qa_write_state checkpoint
printf 'checkpoint OK: %s\n' "$SNIPTALE_QA_RUN_ID"
