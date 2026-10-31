#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
# shellcheck source=../runtime/lib.sh
source "$ROOT/tooling/qa/runtime/lib.sh"
cd "$ROOT"
qa_init "qa:closeout" blocking

commit=true
message=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --no-commit)
      commit=false
      shift
      ;;
    -m | --message)
      message="${2:-}"
      shift 2
      ;;
    *)
      printf 'Unknown argument: %s\n' "$1" >&2
      exit 2
      ;;
  esac
done

if [[ "$commit" == true && -z "$message" ]]; then
  printf 'Commit message required: tooling/qa/wrappers/closeout.sh -m "message"\n' >&2
  printf 'Use --no-commit only for local diagnostic runs.\n' >&2
  exit 2
fi
if [[ -n "$message" ]]; then
  QA_SENSITIVE_VALUES+=("$message")
fi

checkpoint_wrapper="${SNIPTALE_QA_CHECKPOINT_WRAPPER:-tooling/qa/wrappers/checkpoint.sh}"
coverage_wrapper="${SNIPTALE_QA_COVERAGE_WRAPPER:-tooling/qa/wrappers/coverage.sh}"
supply_chain_wrapper="${SNIPTALE_QA_SUPPLY_CHAIN_WRAPPER:-tooling/qa/wrappers/supply-chain.sh}"
build_wrapper="${SNIPTALE_QA_BUILD_WRAPPER:-tooling/qa/wrappers/build.sh}"

qa_require_harness
initial_fingerprint="$(fingerprint_worktree)"
qa_step "checkpoint" "$checkpoint_wrapper"
ran_post_checkpoint_gate=false
if high_risk_files_changed; then
  qa_step "coverage" "$coverage_wrapper"
  ran_post_checkpoint_gate=true
fi
if dependency_files_changed; then
  qa_step "supply chain" "$supply_chain_wrapper"
  ran_post_checkpoint_gate=true
fi
if [[ "$ran_post_checkpoint_gate" == true ]]; then
  qa_step "final checkpoint" "$checkpoint_wrapper"
fi
qa_step "build" "$build_wrapper" --profile dev
qa_fail_if_any "closeout proof summary"

if [[ "$(fingerprint_worktree)" != "$initial_fingerprint" ]]; then
  printf 'Tracked worktree changed while QA proof was running. Rerun closeout.\n' >&2
  exit 1
fi

if [[ "$commit" == true ]]; then
  if ! git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    printf 'Cannot commit: not a git repository.\n' >&2
    exit 1
  fi
  git add -A
  if git diff --cached --name-status | awk -F '\t' '
    $1 != "D" {
      for (column = 2; column <= NF; column++) {
        if ($column ~ /^(\.tmp|tasks)(\/|$)/) found = 1
      }
    }
    END { exit found ? 0 : 1 }
  '; then
    printf 'Refusing to add or modify workspace artifacts from .tmp/** or tasks/**.\n' >&2
    exit 1
  fi
  if [[ "$(fingerprint_worktree)" != "$initial_fingerprint" ]]; then
    printf 'Staged tree does not match the verified worktree.\n' >&2
    exit 1
  fi
  qa_step "git commit" env SNIPTALE_SKIP_GIT_HOOKS=1 git commit -m "$message"
  qa_fail_if_any "closeout commit summary"
fi

printf '\ncloseout OK\n'
