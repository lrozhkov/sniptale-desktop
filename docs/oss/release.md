# Local OSS release

This procedure prepares and verifies Sniptale Desktop release evidence. Local proof wrappers do not publish. The alpha publication workflow creates the source tag and GitHub prerelease only after admission.

## Legal and supply-chain proof

After a reviewed dependency or license-policy change, run:

```bash
tooling/qa/wrappers/licenses.sh --write
tooling/qa/wrappers/release-harness.sh
tooling/qa/wrappers/checkpoint.sh
tooling/qa/wrappers/supply-chain.sh
```

Normal validation uses `licenses.sh --check` and never rewrites tracked legal output. `release.sh` always runs supply-chain/license proof, creates `.tmp/release/sbom.spdx.json`, and produces the release build. [OSS provenance](provenance.md) owns source and dependency-license policy.

## Secret audit

Use a complete, non-shallow Git checkout. If the pinned audit binary is absent, install it with `tooling/qa/installers/install-gitleaks.sh`. Run `tooling/qa/wrappers/audit.sh`. Both the current tree and full history must pass. Reports are fully redacted and remain below ignored `.tmp/qa/audit/**`. Do not publish raw findings.

## Windows package

Run `package-windows.sh` on a Windows-capable builder with an explicit channel and extension origin. Stable production packaging requires a signed EXE and MSI, timestamp/revocation evidence, and a filled `tooling/release/windows/windows-release-evidence-template.json` validated by `verify-windows-release-evidence.py`.

The package evidence set contains:

- `sniptale-native-agent.exe`, `SniptaleNativeAgent-<channel>-<version>.msi`, and `native-host.json`.
- `LICENSE.txt`, `NOTICE.txt`, `THIRD_PARTY_NOTICES.md`, and `THIRD_PARTY_DEPENDENCIES.json`.
- Exact `LICENSES/third-party/**` texts.
- `sbom.spdx.json` and cargo audit/deny reports.
- `provenance.json`, `release-evidence.json`, and `SHA256SUMS`.
- Signature reports and production manual evidence.

## Residual manual acceptance

Automated desktop E2E and Windows smoke remain primary.

Before stable signing, record these residual facts that automation cannot reliably establish:

- Representative Windows 10/11 and Chrome/Edge consent flow.
- Timestamp/revocation status.
- Cold start and capture latency.
- Five-minute mixed-audio recording drops/finalization.
- WPR/WPA or equivalent trace.
- Accessibility Insights or equivalent UIA report.
- Keyboard-only tray/settings operation, high contrast, and mixed DPI.
- Reviewer approval.

## Closeout and rollback

Run `closeout.sh -m "chore(oss): prepare Sniptale Desktop release"` for the coherent verified candidate. Before distribution, rollback is a revert of the candidate. After distribution, preserve published history and issue a forward corrective release. Do not rewrite provenance.

## Unsigned alpha

The first release is `v0.1.0-alpha.1`, with numeric Cargo/MSI version `0.1.0` and runtime channel `dev`. This release supports Manual settings, screenshots, and local MP4 recording. Trusted extension integration is unsupported. No dev-local browser trust flag or marker is shipped. Missing signature is an expected diagnostic limitation, never a production trust claim.

Alpha, beta, and stable currently share the install directory and UpgradeCode, so side-by-side installation is unsupported.

This first alpha is accepted on a clean installation. Uninstall an existing desktop package before installing it. Advance the numeric Cargo/MSI version for the next distributed candidate, including a later alpha or stable. Alpha labels alone do not establish MSI upgrade order.

Use a clean complete checkout. Install the pinned Rust toolchain. Run `tooling/qa/installers/install-release-tools.sh`. Then run `tooling/qa/wrappers/alpha-proof.sh`. The latter produces `.tmp/release/alpha-proof/` with commit-bound gate receipts, legal/supply-chain proof and SBOM.

On Windows with Visual Studio C++ Build Tools, Windows SDK 10.0.26100.0, .NET SDK, Python and rustup, run `powershell.exe -NoProfile -ExecutionPolicy Bypass -File tooling/release/windows/alpha-package.ps1 -QaProof .tmp/release/alpha-proof -Tag v0.1.0-alpha.1`. Output must be a new directory. The same entrypoint runs in Actions.

The alpha links the MSVC CRT statically. [PE import admission](../../tooling/release/windows/verify_windows_dependencies.py) rejects non-system DLL dependencies. Rust and WiX are pinned. Actual MSVC, SDK, runtime and runner versions are recorded. Equal commands and validated contents are supported. Byte-identical MSI builds across mutable runner images are not claimed.

Merge the reviewed change, then dispatch `alpha.yml` from `main` with the tag. Download the immutable `alpha-<commit>-<run>-<attempt>` artifact after the complete run succeeds. Verify its GitHub artifact digest and the extracted bundle with `alpha_artifacts.py verify`. Run `tooling/qa/wrappers/windows-smoke.sh --alpha-artifact <bundle-msi>`. It delegates to the laboratory controller, where a sibling `provenance.json` selects the mandatory alpha release scenarios without baseline/trust fixtures. Collect and verify its receipt, and retain the complete local evidence.

This acceptance must use a clean install, real first-run UI, no prewritten settings or trust fixtures, PNG capture, MP4 finalization, restart and uninstall preserving exports. Development E2E remains separate and cannot admit a release binary.

Export the bounded acceptance with `python3 tooling/release/windows/alpha_acceptance.py <bundle> <lab-run-directory> <artifact-id> <summary-output>`. It verifies the complete local receipt and prints the base64 dispatch input. Dispatch `alpha-publish.yml` from `main` with the source run ID, immutable artifact ID, full commit, tag and bounded base64 acceptance summary.

Admission binds repository, successful workflow/run/attempt, artifact digest, candidate checksums, required Windows cases and receipt hash. The summary is an explicit assertion by the trusted dispatcher that local receipt verification passed. It is not a GitHub-signed attestation.

The publisher downloads the existing artifact as data. It checks the source and complete file inventory before creating a draft. It checks every uploaded hash before publishing a prerelease. It never overwrites an existing tag or release. Inspect a retained draft after partial failure before recovery. Download the public assets and verify their hashes against the draft-tested files after publication.

Release notes identify the unsigned Manual-only scope, Windows image tested, unsupported trusted extension integration, channel replacement behavior and source/run identity. Windows 10 support is not certified by Windows 11 laboratory proof. Stable signing and residual manual acceptance above remain mandatory for production releases.

The hosted alpha builder caches Cargo compilation output. Its key binds the exact Windows runner image and hashes of pinned tool versions and the dependency lockfile. It also binds Cargo configuration, build script, resources, and the shared packaging script, including target and compiler flags. It always cleans the application package and its build script before compiling the current checkout. Dependency cache hits do not replace QA or release evidence.

Local builds use the same invalidation and keep their dependency output in `target`. Release optimization settings remain unchanged. Cargo writes per-crate timing reports under `target/cargo-timings`, and the package log reports compilation elapsed time.
