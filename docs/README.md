# Sniptale Desktop documentation

This index lists every active document. Code, tracked contracts, and machine policy are authoritative for executable behavior; documents own architecture decisions, security assumptions, audit rationale, and operator procedures that are not evident from code.

## Architecture

- [Repository overview](architecture/repository-overview.md) maps runtime modes and authoritative repository inputs.
- [Module ownership](architecture/module-ownership.md) owns product boundaries, state authority, and dependency direction.
- [ADR 0001](architecture/adr/0001-resident-media-worker-ownership.md) records the single-process resident worker boundary.
- [ADR 0002](architecture/adr/0002-dpapi-owner-local-adapters.md) records the two owner-local DPAPI adapters and their trade-off.
- [ADR 0003](architecture/adr/0003-transfer-lifecycle-ownership.md) records shared media-transfer lifecycle ownership.
- [ADR 0005](architecture/adr/0005-dual-settings-authority-and-local-asset-delivery.md) records mutually exclusive extension-managed and Manual authority.

## Engineering and QA

- [Implementation rules](engineering/implementation-rules.md) maps each engineering rule to its deterministic guardrail and separates judgment-only rules.
- [Testing policy](engineering/testing-policy.md) owns required proof families and risk-based test expectations.
- [Unsafe and FFI policy](engineering/unsafe-ffi-policy.md) owns admission requirements for privileged Windows adapters.
- [QA tooling](tooling/qa.md) routes wrappers, evidence, and generated state.

## Security and operations

- [Threat model and trade-offs](security/threat-model.md) owns trust assumptions, mitigations, and residual risks.
- [Data handling](security/data-handling.md) owns sensitive-data storage, retention, and diagnostic-output rules.
- [IPC authorization](security/ipc-auth.md) owns resident-pipe authentication, consent, and controller authorization.
- [Install trust](security/install-trust.md) owns manifest, channel, signature, ACL, and autostart trust.
- [Diagnostics](operations/diagnostics.md) owns support commands, log locations, and triage.

## Open source and release audit

- [OSS provenance](oss/provenance.md) owns project attribution, dependency-license provenance, and tracked legal inputs.
- [Local OSS release](oss/release.md) owns legal generation, release evidence, residual manual acceptance, and rollback.
