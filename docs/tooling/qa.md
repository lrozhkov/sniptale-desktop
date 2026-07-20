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
- `release.sh` always runs checkpoint, coverage, supply-chain/license proof, SPDX SBOM generation, a final checkpoint, and the release build.
- `package-windows.sh` builds the Windows EXE/MSI and always binds manifests, legal payload, provenance, and hashes. Stable production also binds signatures, SBOM, supply-chain reports, and manual evidence.
- `windows-smoke.sh`, `windows-smoke-from-wsl.sh`, and `desktop-e2e-lab.sh` own Windows compile/runtime, installed-package, and isolated guest evidence.

The registry at `tooling/configs/qa/wrappers.json` is the machine owner of canonical wrapper entrypoints and blocking mode. Wrapper source is the owner of exact steps; documentation does not duplicate them.

All canonical QA runs locally. The repository intentionally carries no `.github/workflows/**` files, and `verify_tooling.py` rejects hosted GitHub Actions workflows so heavy checkpoint, coverage, Windows, audit, and release work cannot be moved onto GitHub runners accidentally.

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
