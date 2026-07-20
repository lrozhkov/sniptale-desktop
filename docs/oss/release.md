# Local OSS release

This procedure prepares and verifies local Sniptale Desktop release evidence. It performs no remote, tag, push, upload, store submission, or hosted-policy mutation.

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
