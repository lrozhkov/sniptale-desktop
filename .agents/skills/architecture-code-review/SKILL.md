---
name: architecture-code-review
description: Review high-risk Sniptale Desktop architecture changes without edits. Inspect owner boundaries, contracts, and supplied proof.
---

# Architecture code review

Perform a read-only review. Do not edit files, run QA wrappers, formatters, code generation, or destructive commands.

## Scope and evidence

Receive a bounded diff, acceptance criteria, changed owners, supplied QA results, and known exclusions. State missing inputs explicitly. Do not infer the intended verdict from implementation notes.

Classify findings through AGENTS.md. Block only a security issue, current regression, or acceptance blocker. Each blocker needs a concrete reachable scenario and a minimal correction. Keep unrelated hardening advisory. Flag a correction that would escape the accepted scope.

## Required inputs

Read `AGENTS.md`, `docs/architecture/repository-overview.md`, `docs/architecture/module-ownership.md`, `docs/engineering/implementation-rules.md`, `docs/engineering/testing-policy.md`, and the full [architecture checklist](references/architecture-review-checklist.md). Read the relevant ADR and contract catalogs for the touched seam. Read `docs/engineering/unsafe-ffi-policy.md` for Windows adapters, raw handles, callbacks, hooks, COM, Media Foundation, WASAPI, or unsafe code.

Use the supplied diff and QA evidence. Do not rerun wrappers. Identify missing evidence as residual risk.

## Review boundary

Check owner placement, facade shape, platform-neutral contracts, boundary parsing, state authority, and lifecycle ownership. Check bounded concurrency, failure cleanup, compiler-signal suppressions, deterministic proof, and dependency proof. Identify opportunities for repeatable guardrails. Require Windows-target warning evidence when Windows-only reachability or dead-code cleanup is claimed.

If owner behavior lacks deterministic proof, require fixtures, dev dependencies, fakes, adapters, property/state-machine tests, or a Windows integration lane in the same change.

## Output

List findings first, ordered by severity, with file and line evidence. Then state residual assumptions, missing QA context, and one decision: `Approve`, `Approve with comments`, `Request changes`, or `Needs security review`.

Use `Request changes` only for blockers under the AGENTS.md classification. Explain residual risk and unverified areas separately.
