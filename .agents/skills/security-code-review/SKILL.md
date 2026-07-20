---
name: security-code-review
description: Use for analysis-only Sniptale Desktop security review of high-risk changes touching privacy, privileged Windows APIs, trust boundaries, IPC authorization, installer state, telemetry, transfer, diagnostics, dependency licensing, or signing.
---

# Security code review

Perform a read-only security and privacy review. Do not edit files, run QA wrappers, formatters, code generation, or destructive commands.

## Required inputs

Read `AGENTS.md`, `docs/security/threat-model.md`, the relevant security owner document, `docs/engineering/implementation-rules.md`, and the full [security checklist](references/security-review-checklist.md). Read `docs/engineering/unsafe-ffi-policy.md` for privileged Windows APIs or unsafe code, `contracts/security/redaction-corpus.json` for disclosure surfaces, and `docs/oss/provenance.md` for dependency/license or release-provenance changes.

Use the supplied diff and QA evidence. Do not rerun wrappers; identify missing evidence as residual risk.

## Review boundary

Check trust-boundary parsing, limits, authorization, consent, replay resistance, controller authority, sensitive-data lifetime, redaction, transfer integrity, privileged resource ownership, install/signature state, dependency provenance, legal payload completeness, and fail-closed behavior. For IPC or installer changes, confirm live challenge, controller-lease, manifest-origin, pipe-secret algorithm, and installed provenance paths remain wired.

## Output

List findings first, ordered by severity, with file and line evidence. Then state residual assumptions, missing QA context, and one decision: `Approve`, `Approve with comments`, `Request changes`, or `Needs architecture review`.

Use `Request changes` for P0/P1 findings and P2 findings involving privileged APIs, sensitive data, authorization, installer/signature state, telemetry, transfer integrity, diagnostics redaction, unsafe behavior, dependency trust, licensing, or release provenance.
