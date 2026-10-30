#!/usr/bin/env bash
set -euo pipefail
export PYTHONDONTWRITEBYTECODE=1

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
# shellcheck disable=SC1091
source "$ROOT/tooling/qa/runtime/lib.sh"
cd "$ROOT"
qa_init "qa:desktop-e2e-lab" blocking

lane=""
profile="release"
while [[ $# -gt 0 ]]; do
  case "$1" in
    --lane)
      lane="${2:-}"
      shift 2
      ;;
    --profile)
      profile="${2:-}"
      shift 2
      ;;
    *)
      printf 'Unknown argument: %s\n' "$1" >&2
      exit 2
      ;;
  esac
done

case "$lane" in
  linux-wayland | linux-x11 | windows-11 | all) ;;
  *)
    printf 'Usage: tooling/qa/wrappers/desktop-e2e-lab.sh --lane <linux-wayland|linux-x11|windows-11|all> [--profile dev|release]\n' >&2
    exit 2
    ;;
esac
case "$profile" in
  dev | release) ;;
  *)
    printf 'Unknown profile: %s\n' "$profile" >&2
    exit 2
    ;;
esac

qa_step "desktop E2E doctor" tooling/desktop/e2e-lab doctor
qa_fail_if_any "desktop E2E doctor summary"
report_dir=".tmp/qa-reports/$SNIPTALE_QA_RUN_ID"
mkdir -p "$report_dir"
chmod 700 "$report_dir" 2>/dev/null || true
run_report="$report_dir/desktop-e2e-run.json"
collect_report="$report_dir/desktop-e2e-collect.json"
status_report="$report_dir/desktop-e2e-status.json"
run_id_report="$report_dir/desktop-e2e-run-id.txt"

QA_LIVE_OUTPUT=1 qa_step "desktop E2E run" bash -c \
  'tooling/desktop/e2e-lab run --lane "$1" --build current --profile "$2" > "$3"' \
  sniptale-e2e "$lane" "$profile" "$run_report"
qa_step "desktop E2E run identity" bash -c \
  'python3 -c '\''import json,sys; value=json.load(open(sys.argv[1], encoding="utf-8")); run_id=value.get("run_id"); assert isinstance(run_id, str) and run_id; print(run_id)'\'' "$1" > "$2"' \
  sniptale-e2e "$run_report" "$run_id_report"
run_id=""
if [[ -s "$run_id_report" ]]; then
  run_id="$(<"$run_id_report")"
  qa_step "desktop E2E collect" bash -c \
    'tooling/desktop/e2e-lab collect "$1" > "$2"' \
    sniptale-e2e "$run_id" "$collect_report"
fi
qa_step "desktop E2E status" bash -c \
  'tooling/desktop/e2e-lab status --json > "$1"' \
  sniptale-e2e "$status_report"
if [[ -n "$run_id" && -s "$collect_report" ]]; then
  qa_step "desktop E2E evidence verification" python3 - "$collect_report" "$status_report" "$lane" <<'PY'
import json
import sys

collect_path, status_path, lane = sys.argv[1:]
collect = json.load(open(collect_path, encoding="utf-8"))
status = json.load(open(status_path, encoding="utf-8"))
if collect.get("verified") is not True or collect.get("result") != "pass":
    raise SystemExit("desktop E2E evidence is not a verified pass")
lanes = status.get("lanes", {})
selected = lanes.values() if lane == "all" else [lanes.get(lane, {})]
if any(value.get("running") or value.get("pid") or value.get("state_error") for value in selected):
    raise SystemExit("desktop E2E lane did not teardown cleanly")
PY
fi
qa_fail_if_any "desktop E2E evidence summary"
printf 'desktop E2E OK: %s\n' "$run_id"
