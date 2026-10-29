# Install trust

`install/**` owns runtime identity and repair assessment for executable path, channel, install ID, Native Messaging host name, allowed extension origin, manifests, autostart, signatures, and ACLs.

## Required state

- Stable, beta, and dev use channel-scoped host names and explicit `allowed_origins`; wildcards and unpacked production origins are rejected.
- Runtime consumers use `InstallRuntimeIdentity` instead of composing channel, origin, host name, or install ID independently.
- The manifest resolves to the expected installed executable. Stale shadowing manifests and path drift are repair-required.
- Autostart launches resident mode and never `--native-host`.
- Stable production trust requires signed EXE and MSI evidence. Dev/beta may use the explicit non-production unsigned policy only under their scoped channel and origin.
- The dev alpha compile flavor accepts a missing signature for startup repair assessment only, enabling user-driven Manual operations. Registered-host and pipe-peer authorization still require their existing signature proof; no dev-local browser trust marker is installed. The [threat model](threat-model.md) owns this non-production trade-off.
- Install-directory ACL assessment rejects broad write access by Everyone, Authenticated Users, or BUILTIN\Users.
- Uninstall removes registration and private authorization state but preserves user-published Manual assets.

## Release evidence

Production package evidence binds the EXE, MSI, rendered manifest, project legal files, third-party notices, SPDX SBOM, cargo audit/deny reports, manual Windows evidence, Authenticode reports, provenance, and `SHA256SUMS`. The procedure is owned by [local OSS release](../oss/release.md).

## Trade-offs

WDAC/App Control is deployment guidance for managed environments; AppLocker is secondary. BitLocker is device-storage defense in depth and does not replace protected temp media. CFG/process mitigations remain build/deployment evidence unless a product requirement adds runtime enforcement. [Threat model](threat-model.md) owns residual installer and update risk.
