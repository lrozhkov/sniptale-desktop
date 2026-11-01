---
name: security-code-review
description: Review high-risk Sniptale Desktop security changes without edits. Inspect owner boundaries, contracts, and supplied proof.
---

# Security code review

Perform a read-only security and privacy review. Do not edit files, run QA wrappers, formatters, code generation, or destructive commands.

## Independence and timing

The orchestrator must start a fresh independent read-only reviewer with `fork_turns: "none"`. Do not reuse an implementation agent. Start required closeout review after supplied applicable harness and checkpoint proof passes. Receive the bounded manifest, acceptance criteria, affected consumers, QA results, and known exclusions. Do not receive an intended verdict.

## Scope and evidence

Receive a bounded diff, acceptance criteria, changed owners, supplied QA results, and known exclusions. State missing inputs explicitly. Do not infer the intended verdict from implementation notes.

Classify findings through AGENTS.md. Block only a security issue, current regression, or acceptance blocker. Each blocker needs a concrete reachable scenario and a minimal correction. Keep unrelated hardening advisory. Flag a correction that would escape the accepted scope.

## Required inputs

Read `AGENTS.md`, `docs/security/threat-model.md`, the relevant security owner document, `docs/engineering/implementation-rules.md`, and the full [security checklist](references/security-review-checklist.md). Read `docs/engineering/unsafe-ffi-policy.md` for privileged Windows APIs or unsafe code, `contracts/security/redaction-corpus.json` for disclosure surfaces, and `docs/oss/provenance.md` for dependency/license or release-provenance changes.

Use the supplied diff and QA evidence. Do not rerun wrappers. Identify missing evidence as residual risk.

## Review boundary

Check trust-boundary parsing, limits, authorization, consent, replay resistance, and controller authority. Check sensitive-data lifetime, redaction, transfer integrity, and privileged resource ownership. Check install/signature state, dependency provenance, legal payload completeness, and fail-closed behavior. For IPC or installer changes, confirm live challenge, controller-lease, manifest-origin, pipe-secret algorithm, and installed provenance paths remain wired.

## Output

List findings first, ordered by severity, with file and line evidence. Then state residual assumptions, missing QA context, and one decision: `Approve`, `Approve with comments`, `Request changes`, or `Needs architecture review`.

Use `Request changes` only for blockers under the AGENTS.md classification. Explain residual risk and unverified areas separately.
