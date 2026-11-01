---
name: architecture-code-review
description: Review high-risk Sniptale Desktop architecture changes without edits. Inspect owner boundaries, contracts, and supplied proof.
---

# Architecture code review

Perform a read-only review. Do not edit files, run QA wrappers, formatters, code generation, or destructive commands.

## Independence and timing

Use the review applicability and independence rules in AGENTS.md. The orchestrator must start a fresh read-only reviewer with `fork_turns: "none"`. Do not reuse an implementation agent.

Start required closeout review after green checkpoint and applicable harness proof. Receive the scope manifest, exact diff, preflight result, consumers, QA results, known failures, and exclusions. The manifest must include observable acceptance criteria, non-goals, planned paths, owners, contracts, failure behavior, and required proof. Do not receive intended conclusions.

## Scope and evidence

Receive a bounded diff, acceptance criteria, changed owners, supplied QA results, and known exclusions. State missing inputs explicitly. Do not infer the intended verdict from implementation notes.

Classify findings in the order defined by AGENTS.md. Block only a security issue, current regression, or acceptance blocker. Each blocker needs a reachable scenario, violated criterion or invariant, material impact, and minimal correction. Identify inference explicitly.

Report pre-existing hardening only when it affects a task decision or the user requests it. Flag corrections that escape scope. Do not require hypothetical behavior or unrelated tests.

## Required inputs

Read `AGENTS.md`, `docs/architecture/repository-overview.md`, `docs/architecture/module-ownership.md`, `docs/engineering/implementation-rules.md`, `docs/engineering/testing-policy.md`, and the full [architecture checklist](references/architecture-review-checklist.md). Read the relevant ADR and contract catalogs for the touched seam. Read `docs/engineering/unsafe-ffi-policy.md` for Windows adapters, raw handles, callbacks, hooks, COM, Media Foundation, WASAPI, or unsafe code.

Use the supplied diff and QA evidence. Do not rerun wrappers or request blanket reruns or extra receipt hashes. Identify the exact evidence defect before requesting further proof. Report unavailable required proof as a blocker to completion. Do not treat it as permission to omit the check.

## Review boundary

Check owner placement, facade shape, platform-neutral contracts, boundary parsing, state authority, and lifecycle ownership. Check bounded concurrency, failure cleanup, compiler-signal suppressions, deterministic proof, and dependency proof. Require Windows-target warning evidence when Windows-only reachability or dead-code cleanup is claimed.

Assess cohesion by responsibility, state authority, lifecycle, and current reasons to change. Compare navigation, forwarding layers, public surfaces, and recovery ownership. Recommend splitting, consolidation, or retaining the current shape only from concrete evidence. File counts or hypothetical future features do not establish an architecture defect. A cohesive owner may coordinate state, effects, and recovery through narrow adapters.

If owner behavior lacks deterministic proof, require fixtures, dev dependencies, fakes, adapters, property/state-machine tests, or a Windows integration lane in the same change.

## Proof and corrections

Check the persistence concurrency cases and inapplicability reasons required by AGENTS.md. Check selected test names and supplied checkpoint execution evidence. Include non-interactive writers, recovery, deletion, and identity reuse when applicable. Require controlled ordering rather than timing delays.

For a defect missed by QA, inspect the regression proof, the reason QA missed it, and the deterministic-guard decision. Inspect flagged code before accepting a tool-noise claim. Check exact registry evidence and removal conditions. Temporary debt must not waive blocking checks.

Return findings without applying corrections. Let the orchestrator collect all reviews before one correction batch. State which corrections invalidate semantic review under AGENTS.md. Inspect only affected conclusions during follow-up. Mechanical cleanup alone does not require repeated semantic review.

## Output

List findings first, ordered by severity, with file and line evidence. Then state residual assumptions, missing QA context, and one decision: `Approve`, `Approve with comments`, `Request changes`, or `Needs security review`.

Use `Request changes` only for blockers under the AGENTS.md classification. Distinguish an unavailable required result from an optional observation. Explain residual risk and unverified areas separately.
