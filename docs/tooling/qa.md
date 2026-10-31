# QA tooling

Canonical executable tooling lives under `tooling/**`. Generated logs, state, reports, coverage, release evidence, VM overlays, and audit output live under ignored `.tmp/**`; persistent local desktop-lab inputs live under ignored `.artifacts/**`.

## Managed toolchain

Edit versions only in `tooling/configs/qa/tool-versions.json`, then run `tooling/qa/wrappers/toolchain.sh --sync`. The resolver verifies official release metadata and produces `toolchain-lock.json`, Rust configuration, pinned GitHub Action commits, and hosted Python settings. `--check` is offline; the tooling gate rejects stale generated inputs and incomplete release metadata. Run `--install-rust`, `tooling/qa/installers/install-release-tools.sh`, and `tooling/qa/installers/install-script-tools.sh` to prepare local QA tools. The script installer verifies the cataloged Python and Node.js runtimes, binary checksums, npm lockfile integrity, and hash-locked Python dependencies. If the cataloged Python is absent on Ubuntu 24.04 x64, it extracts the checksum-verified official GitHub Actions Python distribution; it does not compile CPython or require development headers. Tool installation does not build the application or start a VM. QA wrappers and the desktop-lab entrypoint select the cataloged Python; metadata bootstrap and preflight remain usable with the host interpreter before installation.

The version catalog owns the minimum release age. Ordinary stable updates require seven complete days since publication. A younger release requires a reviewed `securityExceptions` entry naming the exact fixed version, an advisory or CVE URL, and the vulnerability reason. The Python exception records the confirmed compromised-CA and tarfile fixes. This rule applies to managed Rust, Cargo lockfile packages, the locked Python/npm checker dependency graphs, QA tools, WiX, SDK packages, Python, and GitHub Actions. Runner OS images and golden OS/browser inventories remain separate platform inputs; a tool update does not silently update those images.

Windows CI and guest builds use `tooling/release/windows/toolchain.ps1` to verify and prepare the same locked WiX and portable SDK packages. Development sessions transfer verified packages over the lab transport and compile without guest network access. Rust components also come from the official hash-bound manifest. Windows smoke uses the cataloged Python version from a checksum-verified embeddable package, so an older golden does not need a Python installation. Updating those pins does not require rebuilding the Windows golden image. WiX 7 is deferred because its new EULA and maintenance-fee terms require a separate decision; the selected WiX 5 line remains explicit in the catalog.

## Wrapper routing

- `preflight.sh` reports repository/toolchain context and runs read-only architecture checks.
- `environment-audit.sh` checks exact cataloged build and QA tool versions against fresh OSV, upstream CPython release commits, and NVD results, and verifies advisory-bound release-age exceptions. It does not build the application or start a VM. Unsupported coverage and platform inputs appear explicitly in its report; empty advisory results are not a claim of complete bundled-dependency coverage.
- `fast.sh` runs the fresh environment audit and script checks concurrently before compilation and desktop-lab start/build/test operations. `script-lints.sh` inventories supported sources and runs Ruff errors/formatting, Pyright, ShellCheck, shfmt, PowerShell parsing/PSScriptAnalyzer, typed ESLint Promise/safety rules, actionlint, zizmor, yamllint, Taplo, and JSON duplicate-key/schema validation with at most four workers. The wrapper also exercises real negative fixtures; the harness includes a known-vulnerable Python rejection test.
- `audit.sh` runs pinned Gitleaks against the current tree and complete Git history. A shallow checkout, unavailable or wrong tool version, finding, malformed baseline, or incomplete report fails the audit.
- `release-harness.sh` validates tooling topology, registries, docs policy, hosted-workflow policy, Python tests, desktop-lab controller tests, and guardrail self-tests. Tooling, hosted-workflow policy, docs-policy, or repo-skill changes require fresh harness state.
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

## Script quality and update policy

All tracked supported scripts and non-ignored new scripts enter the inventory, including extensionless entrypoints. An unsupported interpreter or new TypeScript source fails until an explicit lane is configured. The existing Rust, documentation and license owners remain authoritative; Markdown prose is not reformatted. Rule configurations and the JSON schema registry live in `tooling/configs/qa/`. `verify_script_quality.py` rejects broad exclusions and unexplained suppressions; individual exceptions name the diagnostic and explain the reason.

PowerShell analysis uses explicitly selected rules and a conservative Windows PowerShell 5.1/.NET Framework profile. `windows51-type-profile.json` adds public APIs of the 13 explicitly loaded framework types collected with the pinned PSScriptAnalyzer `TypeDataCollector`; these types are analyzed rather than ignored. The native Windows lane parses every PowerShell source in Windows PowerShell 5.1 and verifies the collected types and members. A negative fixture proves rejection of PowerShell 7 syntax, commands and a .NET Core-only method.

A complete successful script-lint receipt can be reused only for the same Git revision and exact inventoried source/configuration inputs, with every mandatory lane and negative proof present and successful. Rust-only working-tree edits do not invalidate script proof. Script, configuration, catalog or checker changes invalidate it. Installed checker identities are checked against the current dependency locks before reuse. Canonical metadata and lint policy checks remain fresh, and advisory audit results are never reused. In CI, proof transfer is restricted to the same repository, workflow run, attempt and SHA; compilation waits for both fast proof and native Windows PowerShell compatibility.

`renovate.json` configures Mend-hosted Renovate with exact catalog captures, seven complete days of release age, required release timestamps, no automerge and the deferred WiX major-version boundary. CPython uses its official stable-release feed because the generic Python version datasource omits publication timestamps. `renovate_policy.py` independently rejects policy bypasses. Fresh security updates require both a catalog exception and a source-verified proof in `security-update-proofs.json`; the previous version must match the actual base revision, be affected by the advisory, and the selected fixed version must be outside its affected range. A necessary fresh npm dependency of a verified fix needs a separate `requiredBy` proof: the policy checks the publisher's exact fixed-parent metadata, requires the minimum declared child version, and rejects the exception if an aged compatible alternative exists. Labels alone do not authorize an exception. Regenerate derived locks with `toolchain.sh --sync` after catalog changes.

Local validation reads only the checksum-verified official Renovate configuration schema; it does not install or execute the Renovate npm dependency graph. The hosted service runtime is vendor-maintained and is listed as a coverage gap. Hosted updates require the Renovate GitHub App to be enabled for this repository and the configuration to reach the default branch. A local daily audit is scheduled for 10:00 Europe/Moscow; it runs only `environment-audit.sh` and reports actionable changes or source failures.

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

The Windows laboratory uses cataloged Node.js and `playwright-core` packages to attach to the actual guest Chrome and Edge through loopback CDP. The controller verifies official package hashes and release age, validates the archive before extraction, and verifies the extracted client before execution. Persistent browser profiles remain guest-local. The driver does not download or substitute bundled Chromium.

Laboratory Windows builds use a self-signed code-signing certificate trusted only inside the isolated guest (machine Root and current-user TrustedPublisher stores). The private key is non-exportable and remains in that guest; build evidence carries only its public certificate and hash. Baseline/current EXE and MSI signatures and installed executable hashes are verified. This permits testing the real extension's signature requirement without weakening product trust. These certificates and packages are laboratory fixtures and are not public release signing credentials or distributables. The public alpha packaging path remains separate.
