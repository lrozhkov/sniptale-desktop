# QA tooling

Canonical executable tooling lives under `tooling/**`. Generated logs, state, reports, coverage, release evidence, VM overlays, and audit output live under ignored `.tmp/**`; persistent local desktop-lab inputs live under ignored `.artifacts/**`.

## Wrapper routing

- `preflight.sh` reports repository/toolchain context and runs read-only architecture checks.
- `audit.sh` runs pinned Gitleaks against the current tree and complete Git history. A shallow checkout, unavailable or wrong tool version, finding, malformed baseline, or incomplete report fails the audit.
- `release-harness.sh` validates tooling topology, registries, docs policy, hosted-workflow policy, Python tests, shell syntax, desktop-lab controller tests, and guardrail self-tests. Tooling, hosted-workflow policy, docs-policy, or repo-skill changes require fresh harness state.
- `checkpoint.sh` is the in-progress product gate: format, docs/legal policy, static guardrails, Windows-target compilation, clippy, owner tests, and QA runtime tests.
- `coverage.sh` writes aggregate and owner coverage evidence.
- `licenses.sh --check` validates root legal policy and the tracked dependency notice bundle; `licenses.sh --write` is the explicit regeneration path after reviewed dependency changes.
- `supply-chain.sh` runs cargo advisory, source, ban, license-expression, and dependency-license collection checks.
- `closeout.sh -m "message"` runs scoped proof, the dev build, stages the verified tree, and commits. `--no-commit` is diagnostic only.
- `alpha-proof.sh` runs the release harness and canonical release gate, then exports commit-bound QA evidence for the Windows packaging job.
- `release.sh` always runs checkpoint, coverage, supply-chain/license proof, SPDX SBOM generation, a final checkpoint, and the release build.
- `package-windows.sh` builds the Windows EXE/MSI and always binds manifests, legal payload, provenance, and hashes. Stable production also binds signatures, SBOM, supply-chain reports, and manual evidence.
- `windows-smoke.sh`, `windows-smoke-from-wsl.sh`, and `desktop-e2e-lab.sh` own Windows compile/runtime, installed-package, and isolated guest evidence.

The registry at `tooling/configs/qa/wrappers.json` is the machine owner of canonical wrapper entrypoints and blocking mode. Wrapper source is the owner of exact steps; documentation does not duplicate them.

Canonical QA runs locally and through the allowlisted GitHub Actions workflows. `verify_tooling.py` restricts triggers, commands, source checkout, permissions, pinned Actions, and the QA-to-package dependency. Hosted publication is restricted to reviewed `main` and consumes an existing tested package; it never builds. [Release procedure](../oss/release.md) owns alpha preparation and publication.

## Feedback time

Measure the local feedback loop as `checkpoint.sh` followed by `build.sh --profile dev`; measure `closeout.sh` separately because its scoped coverage and supply-chain work depends on the changed owners. Report wall time, individual check time, failure stage, source revision, and cache condition. Distinguish an empty compilation cache, a warm unchanged checkout, and a warm checkout after a source edit. Do not combine these into one percentile. Use at least three comparable warm samples for a median; label single samples explicitly. For a stable workload the initial optimization targets are at least 25% less warm checkpoint wall time, at least 35% less static-proof wall time, and no regression in the warm dev-build median beyond 1 second. These are investigation targets, not permission to skip QA or release gates.

`checkpoint.sh` runs the independent command catalog in `tooling/qa/runtime/checkpoint_checks.py` with at most four workers. Every independent command runs, including after another check fails; each check retains its result and sanitized output. A failed static proof stops the checkpoint before compilation and cannot write passed checkpoint state. Cargo compilation, clippy, and Rust tests remain sequential. Set `SNIPTALE_QA_STATIC_JOBS=1` for a comparable sequential measurement or a constrained machine; accepted values are 1 through 4. The default is four. Compilation caches do not contain accepted QA state.

Local checkpoint and dev-build wrappers enable Cargo incremental compilation by default. Each profile keeps its own cache; the existing target-budget wrapper still enforces the disk budget. CI, coverage, alpha proof, and release builds retain the non-incremental default. An explicit `CARGO_INCREMENTAL` value takes precedence, so `CARGO_INCREMENTAL=0` remains available for comparison or diagnosis. First-time cache preparation costs a full compilation; measure subsequent edits separately. A checkpoint also verifies that its source inputs still match its starting receipt before writing passed state.

Use `python3 -m tooling.qa.runtime.metrics --run-id <after-id> --before-run-id <before-id>` to compare retained successful receipts of the same wrapper. The report distinguishes group wall time from overlapping child work and does not sum them twice. Read the source revision and changed workload before interpreting a speedup; the report is advisory and never admits a build. Keep benchmark receipts and logs under `.tmp/qa-benchmarks/`; the normal retention policy still applies to the original wrapper records.

Hosted Linux proof restores only Cargo compilation output and invalidates the application package for host, Windows cross-check, and coverage targets before running the complete proof. It also removes application outputs before saving the dependency cache. The cache key binds runner image and architecture, pinned tools, lockfile, Cargo/build/resource inputs, and the cache/coverage/workflow configuration; broad fallback keys are forbidden. Dependency outputs may be reused across application edits, but QA, coverage data, SBOM, audits, and commit-bound evidence are regenerated. Local Cargo output remains warm across wrapper calls; no dependency cleanup is added to the local edit loop.

For Windows acceptance, report controller wall time, guest scenario durations, and the difference as environment overhead; do not equate that difference with removable waiting. Any later optimization of VM preparation must keep a fresh overlay, real installation and first run, exact artifact hashes, the complete scenario set, verified evidence, and teardown proof.

## Evidence

- Wrapper records: `.tmp/qa-observability/runs/YYYY-MM-DD/`.
- Sanitized logs: `.tmp/qa-logs/YYYY-MM-DD/`.
- Machine reports: `.tmp/qa-reports/`.
- Freshness state: `.tmp/qa/state/`.
- Coverage: `.tmp/qa/coverage/`.
- Supply-chain and license reports: `.tmp/release/supply-chain/`.
- Redacted secret-audit reports: `.tmp/qa/audit/`.
- SPDX SBOM: `.tmp/release/sbom.spdx.json`.
- Windows package and smoke evidence: `.tmp/release/windows-<channel>/`.
- Desktop E2E receipts, JUnit, and collected evidence: `.tmp/qa/desktop-e2e-lab/`.

## Gate ownership

[Implementation rules](../engineering/implementation-rules.md) maps product rules to deterministic guardrails. [Testing policy](../engineering/testing-policy.md) owns required proof families. [Local OSS release](../oss/release.md) owns release and residual manual acceptance.
