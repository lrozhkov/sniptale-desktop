---
name: architecture-code-review
description: Use for analysis-only Sniptale Desktop architecture review of high-risk changes touching owner topology, runtime ownership, shared contracts, state authority, parser contracts, deterministic proof, dependencies, or maintainability seams.
---

# Architecture code review

Perform a read-only review. Do not edit files, run QA wrappers, formatters, code generation, or destructive commands.

## Required inputs

Read `AGENTS.md`, `docs/architecture/repository-overview.md`, `docs/architecture/module-ownership.md`, `docs/engineering/implementation-rules.md`, `docs/engineering/testing-policy.md`, and the full [architecture checklist](references/architecture-review-checklist.md). Read the relevant ADR and contract catalogs for the touched seam. Read `docs/engineering/unsafe-ffi-policy.md` for Windows adapters, raw handles, callbacks, hooks, COM, Media Foundation, WASAPI, or unsafe code.

Use the supplied diff and QA evidence. Do not rerun wrappers; identify missing evidence as residual risk.

## Review boundary

Check owner placement, facade shape, platform-neutral contracts, boundary parsing, state authority, lifecycle ownership, bounded concurrency, failure cleanup, compiler-signal suppressions, deterministic proof, dependency proof, and repeatable guardrail opportunities. Require Windows-target warning evidence when Windows-only reachability or dead-code cleanup is claimed.

If owner behavior lacks deterministic proof, require fixtures, dev dependencies, fakes, adapters, property/state-machine tests, or a Windows integration lane in the same change.

## Output

List findings first, ordered by severity, with file and line evidence. Then state residual assumptions, missing QA context, and one decision: `Approve`, `Approve with comments`, `Request changes`, or `Needs security review`.

Use `Request changes` for P0/P1 findings and P2 findings that cross owner boundaries, shared contracts, state authority, parser contracts, lifecycle ownership, deterministic QA, or broad topology.
