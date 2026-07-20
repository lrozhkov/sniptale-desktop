# Sniptale Desktop installer

`installer/wix/SniptaleNativeAgent.wxs` is the parameterized per-user WiX v4 package. The supported entrypoint is `tooling/qa/wrappers/package-windows.sh`; direct WiX calls are implementation details checked by release guardrails.

## Inputs

- `NativeAgentExe`: built and signed `sniptale-native-agent.exe`.
- `NativeHostManifest`: rendered channel-specific `native-host.json`.
- `NativeHostName`: channel-scoped Native Messaging host name.
- `ProductVersion`: MSI version.
- `ProjectLicense`, `ProjectNotice`, `ThirdPartyNotices`, and `ThirdPartyDependencies`: verified OSS legal payload.
- `AppIcon`: tracked Windows icon.
- `WixToolset.UI.wixext`: WiX UI extension matched to the pinned WiX version.

Run packaging with an exact extension origin:

```bash
tooling/qa/wrappers/package-windows.sh \
  --channel stable \
  --extension-origin chrome-extension://<production-extension-id>/
```

The wrapper always requires fresh `checkpoint.sh` and dependency-license state, builds the Windows target, renders the manifest, bundles legal files, and writes `release-evidence.json`, `SHA256SUMS`, and `provenance.json`. Stable production additionally requires fresh supply-chain/SPDX state, Authenticode certificate, timestamp, revocation evidence when supported, validated manual evidence, and copied SBOM/audit/signature reports. Beta/dev manifest templates remain channel-scoped and may use the explicit non-production unsigned policy.

The MSI installs the executable, `native-host.json`, `LICENSE.txt`, `NOTICE.txt`, `THIRD_PARTY_NOTICES.md`, and `THIRD_PARTY_DEPENDENCIES.json`. [Local OSS release](../docs/oss/release.md) owns the release procedure and residual manual acceptance.
