# QA tooling

Canonical executable tooling lives under `tooling/**`. Generated logs, state, reports, coverage, release evidence, VM overlays, and audit output live under ignored `.tmp/**`. Persistent local desktop-lab inputs live under ignored `.artifacts/**`.

## Managed toolchain

Edit versions only in `tooling/configs/qa/tool-versions.json`, then run `tooling/qa/wrappers/toolchain.sh --sync`. The resolver verifies official release metadata and produces `toolchain-lock.json`, Rust configuration, pinned GitHub Action commits, and hosted Python settings. `--check` is offline. The tooling gate rejects stale generated inputs and incomplete release metadata.

Use `tooling/qa/wrappers/toolchain.sh --sync-cargo` when only Cargo.lock changes. It refreshes Cargo release metadata and validates the complete candidate. It preserves unchanged platform tools and workflow pins. Failed validation or a concurrent lock edit prevents publication.

Run `--install-rust`, `tooling/qa/installers/install-release-tools.sh`, and `tooling/qa/installers/install-script-tools.sh` to prepare local QA tools. The script installer verifies the cataloged Python and Node.js runtimes, binary checksums, npm lockfile integrity, and hash-locked Python dependencies.

If the cataloged Python is absent on Ubuntu 24.04 x64, it extracts the checksum-verified official GitHub Actions Python distribution. It does not compile CPython or require development headers.

Tool installation does not build the application or start a VM. QA wrappers and the desktop-lab entrypoint select the cataloged Python. Metadata bootstrap and preflight remain usable with the host interpreter before installation.

The version catalog owns the minimum release age. Ordinary stable updates require seven complete days since publication.

A younger release requires a reviewed `securityExceptions` entry naming the exact fixed version, an advisory or CVE URL, and the vulnerability reason. The Python exception records the confirmed compromised-CA and tarfile fixes.

This rule applies to managed Rust, Cargo lockfile packages, the locked Python/npm checker dependency graphs, QA tools, WiX, SDK packages, Python, and GitHub Actions.

Runner OS images and golden OS/browser inventories remain separate platform inputs. A tool update does not silently update those images.

Windows CI and guest builds use `tooling/release/windows/toolchain.ps1` to verify and prepare the same locked WiX and portable SDK packages. Development sessions transfer verified packages over the lab transport and compile without guest network access. Rust components also come from the official hash-bound manifest.

Windows smoke uses the cataloged Python version from a checksum-verified embeddable package, so an older golden does not need a Python installation. Updating those pins does not require rebuilding the Windows golden image. WiX 7 is deferred because its new EULA and maintenance-fee terms require a separate decision. The selected WiX 5 line remains explicit in the catalog.

## Wrapper routing

- `preflight.sh` reports repository/toolchain context and runs read-only architecture checks.
- `environment-audit.sh` checks exact cataloged build and QA tool versions against fresh OSV, upstream CPython release commits, and NVD results, and verifies advisory-bound release-age exceptions. It does not build the application or start a VM. Unsupported coverage and platform inputs appear explicitly in its report. Empty advisory results are not a claim of complete bundled-dependency coverage.
- `fast.sh` runs the fresh environment audit and script checks concurrently before compilation and desktop-lab start/build/test operations. `script-lints.sh` inventories supported sources and uses at most four workers. Checks include Ruff errors/formatting, Pyright, ShellCheck, shfmt, and PowerShell parsing/PSScriptAnalyzer. They also include typed ESLint Promise/safety rules, actionlint, zizmor, yamllint, Taplo, and JSON duplicate-key/schema validation. The wrapper also exercises real negative fixtures. The harness includes a known-vulnerable Python rejection test.
- `audit.sh` runs pinned Gitleaks against the current tree and complete Git history. A shallow checkout, unavailable or wrong tool version, finding, malformed baseline, or incomplete report fails the audit.
- `release-harness.sh` validates tooling topology, registries, docs policy, hosted-workflow policy, Python tests, desktop-lab controller tests, and guardrail self-tests. Tooling, hosted-workflow policy, docs-policy, or repo-skill changes require fresh harness state.
- `checkpoint.sh` is the in-progress product gate: format, docs/legal policy, static guardrails, Windows-target compilation, clippy, owner tests, and QA runtime tests.
- `coverage.sh` requires a fresh checkpoint before instrumented compilation and writes aggregate and owner coverage evidence.
- `licenses.sh --check` validates root legal policy and the tracked dependency notice bundle. `licenses.sh --write` is the explicit regeneration path after reviewed dependency changes.
- `supply-chain.sh` runs cargo advisory, source, ban, license-expression, and dependency-license collection checks.
- `closeout.sh -m "message"` runs scoped proof, the dev build, stages the verified tree, and commits. `--no-commit` is diagnostic only.
- `alpha-proof.sh` runs the release harness and canonical release gate, then exports commit-bound QA evidence for the Windows packaging job.
- `release.sh` always runs checkpoint, coverage, supply-chain/license proof, SPDX SBOM generation, a final checkpoint, and the release build.
- `package-windows.sh` builds the Windows EXE/MSI and always binds manifests, legal payload, provenance, and hashes. Stable production also binds signatures, SBOM, supply-chain reports, and manual evidence.
- `windows-smoke.sh`, `windows-smoke-from-wsl.sh`, and `desktop-e2e-lab.sh` own Windows compile/runtime, installed-package, and isolated guest evidence.

The registry at `tooling/configs/qa/wrappers.json` is the machine owner of canonical wrapper entrypoints and blocking mode. Wrapper source is the owner of exact steps. Documentation does not duplicate them.

Canonical QA runs locally and through the allowlisted GitHub Actions workflows. `verify_tooling.py` restricts triggers, commands, source checkout, permissions, pinned Actions, and the QA-to-package dependency. Hosted publication is restricted to reviewed `main` and consumes an existing tested package. It never builds. [Release procedure](../oss/release.md) owns alpha preparation and publication.

## Script quality and update policy

All tracked supported scripts and non-ignored new scripts enter the inventory, including extensionless entrypoints. An unsupported interpreter or new TypeScript source fails until an explicit lane is configured. The existing Rust, documentation and license owners remain authoritative. Markdown prose is not reformatted. Rule configurations and the JSON schema registry live in `tooling/configs/qa/`. `verify_script_quality.py` rejects broad exclusions and unexplained suppressions. Individual exceptions name the diagnostic and explain the reason.

PowerShell analysis uses explicitly selected rules and a conservative Windows PowerShell 5.1/.NET Framework profile. `windows51-type-profile.json` adds public APIs of the explicitly loaded framework types collected with the pinned PSScriptAnalyzer `TypeDataCollector`. These types are analyzed rather than ignored. The native Windows lane parses every PowerShell source in Windows PowerShell 5.1 and verifies the collected types and members. A negative fixture proves rejection of PowerShell 7 syntax, commands and a .NET Core-only method.

A complete successful script-lint receipt requires the same Git revision and exact inventoried source/configuration inputs for reuse. Every mandatory lane and negative proof must be present and successful.

Rust-only working-tree edits do not invalidate script proof. Script, configuration, catalog or checker changes invalidate it.

Installed checker identities are checked against the current dependency locks before reuse.

Canonical metadata and lint policy checks remain fresh, and advisory audit results are never reused.

In CI, proof transfer is restricted to the same repository, workflow run, attempt and SHA. Compilation waits for both fast proof and native Windows PowerShell compatibility.

`renovate.json` configures Mend-hosted Renovate with exact catalog captures, seven complete days of release age, required release timestamps, no automerge and the deferred WiX major-version boundary. CPython uses its official stable-release feed because the generic Python version datasource omits publication timestamps. `renovate_policy.py` independently rejects policy bypasses.

Fresh security updates require both a catalog exception and a source-verified proof in `security-update-proofs.json`. The previous version must match the actual base revision, be affected by the advisory, and the selected fixed version must be outside its affected range.

A necessary fresh npm dependency of a verified fix needs a separate `requiredBy` proof. The policy checks the publisher's exact fixed-parent metadata and requires the minimum declared child version. It rejects the exception if an aged compatible alternative exists. Labels alone do not authorize an exception. Regenerate derived locks with `toolchain.sh --sync` after catalog changes.

Local validation reads only the checksum-verified official Renovate configuration schema. It does not install or execute the Renovate npm dependency graph. The hosted service runtime is vendor-maintained and is listed as a coverage gap.

Hosted updates require the Renovate GitHub App to be enabled for this repository and the configuration to reach the default branch. A local daily audit is scheduled for 10:00 Europe/Moscow. It runs only `environment-audit.sh` and reports actionable changes or source failures.

## Feedback time

Measure the local feedback loop as `checkpoint.sh` followed by `build.sh --profile dev`. Measure `closeout.sh` separately because its scoped coverage and supply-chain work depends on the changed owners. Report wall time, individual check time, failure stage, source revision, and cache condition. Distinguish an empty compilation cache, a warm unchanged checkout, and a warm checkout after a source edit. Do not combine these into one percentile. Use at least three comparable warm samples for a median. Label single samples explicitly.

For a stable workload, initial optimization targets require at least 25% less warm checkpoint wall time. They require at least 35% less static-proof wall time. The warm dev-build median must not regress by more than 1 second. These are investigation targets, not permission to skip QA or release gates.

`checkpoint.sh` runs the independent command catalog in `tooling/qa/runtime/checkpoint_checks.py` with at most four workers. Every independent command runs, including after another check fails. Each check retains its result and sanitized output. A failed static proof stops the checkpoint before compilation and cannot write passed checkpoint state.

Cargo compilation, clippy, and Rust tests remain sequential. Set `SNIPTALE_QA_STATIC_JOBS=1` for a comparable sequential measurement or a constrained machine. Accepted values are 1 through 4. The default is four. Compilation caches do not contain accepted QA state.

Local checkpoint and dev-build wrappers enable Cargo incremental compilation by default. Each profile keeps its own cache. The existing target-budget wrapper still enforces the disk budget. CI, coverage, alpha proof, and release builds retain the non-incremental default. An explicit `CARGO_INCREMENTAL` value takes precedence, so `CARGO_INCREMENTAL=0` remains available for comparison or diagnosis. First-time cache preparation costs a full compilation. Measure subsequent edits separately. A checkpoint also verifies that its source inputs still match its starting receipt before writing passed state.

Use `python3 -m tooling.qa.runtime.metrics --run-id <after-id> --before-run-id <before-id>` to compare retained successful receipts of the same wrapper. The report distinguishes group wall time from overlapping child work and does not sum them twice. Read the source revision and changed workload before interpreting a speedup. The report is advisory and never admits a build. Keep benchmark receipts and logs under `.tmp/qa-benchmarks/`. The normal retention policy still applies to the original wrapper records.

Hosted Linux proof restores only Cargo compilation output and invalidates the application package for host, Windows cross-check, and coverage targets before running the complete proof. It also removes application outputs before saving the dependency cache. The cache key binds runner image and architecture, pinned tools, lockfile, Cargo/build/resource inputs, and the cache/coverage/workflow configuration. Broad fallback keys are forbidden. Dependency outputs may be reused across application edits, but QA, coverage data, SBOM, audits, and commit-bound evidence are regenerated.

Local Cargo output remains warm across wrapper calls. No dependency cleanup is added to the local edit loop.

For Windows acceptance, report controller wall time, guest scenario durations, and the difference as environment overhead. Do not equate that difference with removable waiting. Any later VM preparation optimization must preserve all acceptance requirements. These include a fresh overlay, real installation and first run, exact artifact hashes, and the complete scenario set. Verified evidence and teardown proof remain required.

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

Laboratory Windows builds use a self-signed code-signing certificate trusted only inside the isolated guest (machine Root and current-user TrustedPublisher stores). The private key is non-exportable and remains in that guest. Build evidence carries only its public certificate and hash. Baseline/current EXE and MSI signatures and installed executable hashes are verified. This permits testing the real extension's signature requirement without weakening product trust.

These certificates and packages are laboratory fixtures and are not public release signing credentials or distributables. The public alpha packaging path remains separate.

## English documentation

The documentation gate applies the repository [asd-ste100 skill](../../.agents/skills/asd-ste100/SKILL.md) to every registered authored Markdown document, including AGENTS.md and skills. It blocks structural errors without a baseline. Lexical consistency, passive voice, and compound-tense findings remain visible advice. Reviewers must check semantics and the stricter instruction limit.

The gate preserves code, frontmatter, and link targets. The legal generator owns the exact third-party notice bundle. License texts remain unchanged. Release harness and checkpoint use this same gate locally and in CI. The harness also executes the checker self-tests and integration negatives.

## Release analysis

`tooling/qa/wrappers/static-analysis.sh` runs jscpd and CodeQL before the other release gates. The alpha proof uses the same release wrapper locally and in CI. Installation uses the common version catalog, official release metadata, minimum release age, and verified archive hashes. Installed file hashes detect changed tool contents. The CodeQL bundle uses a private temporary tool cache outside the source root. This prevents bundled externs from becoming application input. Both tools are QA-only and remain outside the Cargo redistribution closure.

jscpd scans the exact tracked and non-ignored source inventory, including tests and Windows code. It compares blocks of at least 15 lines and 100 tokens, without comment tokens. Findings are advisory maintenance evidence. A duplication percentage does not force shared abstractions or mechanical decomposition. An empty scan, failed tool, malformed report, or stale registry entry blocks release.

CodeQL analyzes Rust with the Windows MSVC target and all features, plus Python, JavaScript, and GitHub Actions. It uses the bundled security-and-quality suites with two threads and a 6144 MiB memory setting. Rust extraction uses build mode `none`. It still executes build scripts and macro compilation. The gate checks extraction coverage for every inventoried Rust file, including build.rs and Windows adapters. These checks supplement existing Clippy, owner, parser, unsafe, dependency, and workflow checks.

CodeQL warnings and errors block release unless an exact reviewed tool-noise entry applies. Temporary debt cannot suppress a blocking CodeQL finding. Missing rules, unsuccessful invocations, malformed reports, and source changes during analysis fail closed. The wrapper retains SARIF, JSON findings, command logs, and timings under `.tmp/qa/static-analysis/<run-id>/`. Release proof binds the analysis gate to the candidate commit.

## Tool-noise and technical debt

`tooling/configs/qa/exception-registry.json` remains the central registry. Its `exceptions` list owns existing architectural exceptions. Its `technicalDebt` list owns exact analyzer findings. Do not copy the same finding into another baseline.

Every analyzer record requires:

- A unique ID and a classification: `tool-noise` or `temporary-debt`.
- An owner, rationale, and concrete risk.
- The exact tool, rule, finding fingerprint, and source paths.
- A review date, removal condition, and target action.
- Tracked proof references.

The fingerprint binds the source paths, bounds, snippets, and rule. Moving or changing the relevant text requires renewed review. Lightweight checks compare source bounds and snippet hashes during every tooling gate. They reject broad scopes, missing paths, expired dates, duplicate identities, and missing metadata. Release analysis also rejects findings that no longer exist.

Use `static-analysis.sh --review-run <run-id>` after a registry-only correction. This mode verifies the complete raw report inventory and hashes. It requires the same Git revision, source files, dependencies, tool pins, and analyzer settings. It permits registry and Markdown changes without another scan. It never reuses an accepted policy verdict. The normal release wrapper always requests a fresh scan. New findings remain visible. Debt records describe real deferred work. Tool-noise records explain why a precise heuristic finding does not require a code change.

Use owner boundaries and behavior to decide whether duplication needs correction. Independent scenario setup and the two DPAPI authorities must not become shared product authority solely to satisfy jscpd. Fix genuine defects before registration. Do not classify a real security defect as tool-noise.

## Cost and limits

This repository is public. Standard GitHub-hosted runners have no usage charge for public repositories. CodeQL use follows its published terms. Runtime, memory, storage, and artifact retention still have costs. Heavy analysis belongs to release proof rather than each edit checkpoint.

- [GitHub Actions billing](https://docs.github.com/en/billing/concepts/product-billing/github-actions) defines hosted usage terms.
- [CodeQL CLI availability](https://docs.github.com/en/code-security/reference/code-scanning/codeql/codeql-cli) defines repository eligibility.
- [CodeQL Rust extraction](https://docs.github.com/en/code-security/reference/code-scanning/codeql/build-options-for-compiled-languages) explains the build mode.
- [jscpd](https://github.com/kucherenko/jscpd) owns detector behavior.

The environment audit checks the exact jscpd package through OSV. CodeQL bundle advisories require upstream review because no supported exact-version OSV package identity covers the complete bundle. An empty advisory response does not prove bundled-component safety.
