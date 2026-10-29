# Threat model and security trade-offs

## Protected assets

Protected assets are captured pixels, video/audio, telemetry signals, Manual profile and destination paths, temporary media keys, pipe secrets, controller authority, extension origin, installer provenance, and diagnostic output.

## Trust boundaries

- Browser Native Messaging input is untrusted until origin, frame size, protocol version, and payload parsing succeed.
- Same-user local processes are untrusted resident-pipe clients until challenge authentication, origin checks, native consent, session bounds, and controller lease checks succeed.
- Windows APIs, registry state, manifests, filesystem metadata, process facts, and signatures are untrusted adapter output until owner validation succeeds.
- Temporary media stays protected and owner-local. Manual publication intentionally crosses into plaintext at the user-selected destination.
- Logs and support bundles are disclosure boundaries and receive redacted summaries only.

## Main controls

[IPC authorization](ipc-auth.md), [install trust](install-trust.md), [data handling](data-handling.md), owner-local unsafe policy, bounded queues, protected temp media, transfer integrity, and deterministic negative tests form the control set. The exact implementation and evidence live in code, contracts, machine catalogs, and wrapper receipts.

## Accepted trade-offs and residual risk

- The resident runtime is one process. Owner boundaries reduce accidental coupling but do not contain a process-wide memory corruption or privileged adapter compromise. [ADR 0001](../architecture/adr/0001-resident-media-worker-ownership.md) records the extraction boundary.
- Current-user DPAPI and pipe ACLs do not defend a fully compromised Windows user session. Challenge authentication and native consent reduce unauthorized same-user automation but cannot restore trust after account compromise.
- ExtensionManaged treats the explicitly allowed extension and active controller lease as product authority. A compromised allowed extension can request operations within the accepted protocol until the lease or trust is revoked.
- Parent-process and executable provenance help consent and diagnostics but are not a browser allowlist. Browser update paths make a strict parent hash policy operationally brittle.
- Manual output is plaintext after atomic publication and inherits destination ACLs and backup/sync behavior. Network, removable, and reparse destinations are rejected, but the user controls the final local directory.
- Stable production packages require Authenticode evidence. Dev and beta channels may be unsigned only with channel-scoped host names, explicit origins, and non-production trust policy.
- The explicitly compiled dev alpha flavor permits a missing signature in startup repair assessment so user-driven Manual capture can run. Invalid or unknown signatures still fail that assessment. This exception does not enable dev-local browser trust, change registered-host or pipe-peer signature authorization, or apply to stable/beta. Unsigned package health remains non-production; trusted extension integration is unsupported.
- Recording telemetry uses privileged low-level hooks. It is opt-in, session-frozen, disabled in Manual v1, and sanitized to counts/labels; hook presence remains observable to the operating system and other privileged software.
- Diagnostics favor categories over identifiers and raw context. This limits forensic detail by design.
- No automatic updater is owned by this repository. Release provenance and installer repair do not provide update delivery or rollback orchestration.
