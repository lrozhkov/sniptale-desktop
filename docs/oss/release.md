# Local OSS release

This procedure prepares and verifies Sniptale Desktop release evidence. Local proof wrappers do not publish; the alpha publication workflow creates the source tag and GitHub prerelease only after admission.

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

From a complete, non-shallow Git checkout, install the pinned audit binary with `tooling/qa/installers/install-gitleaks.sh` when it is not already available, then run `tooling/qa/wrappers/audit.sh`. Both the current tree and full history must pass. Reports are fully redacted and remain below ignored `.tmp/qa/audit/**`; do not publish raw findings.

## Windows package

Run `package-windows.sh` on a Windows-capable builder with an explicit channel and extension origin. Stable production packaging requires a signed EXE and MSI, timestamp/revocation evidence, and a filled `tooling/release/windows/windows-release-evidence-template.json` validated by `verify-windows-release-evidence.py`.

The package evidence set includes `sniptale-native-agent.exe`, `SniptaleNativeAgent-<channel>-<version>.msi`, `native-host.json`, `LICENSE.txt`, `NOTICE.txt`, `THIRD_PARTY_NOTICES.md`, `THIRD_PARTY_DEPENDENCIES.json`, exact `LICENSES/third-party/**` texts, `sbom.spdx.json`, cargo audit/deny reports, `provenance.json`, `release-evidence.json`, `SHA256SUMS`, signature reports, and production manual evidence.

## Residual manual acceptance

Automated desktop E2E and Windows smoke remain primary. Before stable signing, record only residual facts that automation cannot establish reliably: representative Windows 10/11 and Chrome/Edge consent flow, timestamp/revocation status, cold start and capture latency, five-minute mixed-audio recording drops/finalization, WPR/WPA or equivalent trace, Accessibility Insights or equivalent UIA report, keyboard-only tray/settings operation, high contrast, mixed DPI, and reviewer approval.

## Closeout and rollback

Run `closeout.sh -m "chore(oss): prepare Sniptale Desktop release"` for the coherent verified candidate. Before distribution, rollback is a revert of the candidate. After distribution, preserve published history and issue a forward corrective release; do not rewrite provenance.

## Unsigned alpha

The first release is `v0.1.0-alpha.1`, with numeric Cargo/MSI version `0.1.0` and runtime channel `dev`. This release supports Manual settings, screenshots, and local MP4 recording. Trusted extension integration is unsupported; no dev-local browser trust flag or marker is shipped. Missing signature is an expected diagnostic limitation, never a production trust claim. Alpha, beta, and stable currently share the install directory and UpgradeCode, so side-by-side installation is unsupported. This first alpha is accepted on a clean installation; uninstall an existing desktop package before installing it. Advance the numeric Cargo/MSI version for the next distributed candidate, including a later alpha or stable; alpha labels alone do not establish MSI upgrade order.

From a clean complete checkout, install the pinned Rust toolchain and run `tooling/qa/installers/install-release-tools.sh`, then `tooling/qa/wrappers/alpha-proof.sh`. The latter produces `.tmp/release/alpha-proof/` with commit-bound gate receipts, legal/supply-chain proof and SBOM. On Windows with Visual Studio C++ Build Tools, Windows SDK 10.0.26100.0, .NET SDK, Python and rustup, run `powershell.exe -NoProfile -ExecutionPolicy Bypass -File tooling/release/windows/alpha-package.ps1 -QaProof .tmp/release/alpha-proof -Tag v0.1.0-alpha.1`. Output must be a new directory. The same entrypoint runs in Actions. Rust and WiX are pinned; actual MSVC, SDK, runtime and runner versions are recorded. Equal commands and validated contents are supported; byte-identical MSI builds across mutable runner images are not claimed.

Merge the reviewed change, then dispatch `alpha.yml` from `main` with the tag. Download the immutable `alpha-<commit>-<run>-<attempt>` artifact after the complete run succeeds. Verify its GitHub artifact digest and the extracted bundle with `alpha_artifacts.py verify`. Run `tooling/qa/wrappers/windows-smoke.sh --alpha-artifact <bundle-msi>`; it delegates to the laboratory controller, where a sibling `provenance.json` selects the mandatory alpha release scenarios without baseline/trust fixtures. Collect and verify its receipt, and retain the complete local evidence. This acceptance must use a clean install, real first-run UI, no prewritten settings or trust fixtures, PNG capture, MP4 finalization, restart and uninstall preserving exports. Development E2E remains separate and cannot admit a release binary.

Export the bounded acceptance with `python3 tooling/release/windows/alpha_acceptance.py <bundle> <lab-run-directory> <artifact-id> <summary-output>`; it verifies the complete local receipt and prints the base64 dispatch input. Dispatch `alpha-publish.yml` from `main` with the source run ID, immutable artifact ID, full commit, tag and bounded base64 acceptance summary. Admission binds repository, successful workflow/run/attempt, artifact digest, candidate checksums, required Windows cases and receipt hash. The summary is an explicit assertion by the trusted dispatcher that local receipt verification passed; it is not a GitHub-signed attestation. The publisher downloads the existing artifact as data, verifies its source and complete file inventory, creates a draft, verifies every uploaded hash, and only then publishes a prerelease. It never overwrites an existing tag or release. Inspect a retained draft after partial failure before recovery. Download the public assets and verify their hashes against the draft-tested files after publication.

Release notes identify the unsigned Manual-only scope, Windows image tested, unsupported trusted extension integration, channel replacement behavior and source/run identity. Windows 10 support is not certified by Windows 11 laboratory proof. Stable signing and residual manual acceptance above remain mandatory for production releases.
