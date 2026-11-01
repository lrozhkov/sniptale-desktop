---
name: topology-plan-review
description: Review plans and completed candidates for large Rust owner moves. Check consumers, collisions, contracts, and proof without edits.
---

# Topology plan review

Use this skill for a large owner move or a rename-heavy boundary change. Choose planning, candidate review, or wrapper diagnosis.

Read AGENTS.md, the module ownership map, implementation rules, and testing policy. Receive the scope manifest and the selected review mode. Include exact scope, non-goals, old and new paths, file modes, content digests, consumers, ownership, and contract effects. Require collision handling, rollback, a proof map, preflight output, known failures, and supplied QA results. Use explicit not-run statuses for planning proof. Do not receive intended conclusions.

Use a fresh independent read-only agent with `fork_turns: "none"` for each required review. Do not reuse planning or implementation agents for candidate review.

In planning mode, identify predictable omissions before implementation. No green checkpoint is required for a plan. In candidate mode, compare the complete diff with the plan and supplied applicable green proof. In diagnosis mode, inspect supplied output without executing the wrapper.

Check every moved consumer, explicit re-export, fixture, source-reading check, catalog, build input, and documentation link. Inspect the complete selected scope rather than a representative sample. Decompose by independent responsibility, state, lifecycle, and current reasons to change. Compare navigation, forwarding layers, public surfaces, state authority, effects, and recovery. Reject extra layers created only to satisfy counters. Reject consolidation that removes a required boundary.

Do not create owners for hypothetical future features.

Check persistence concurrency proof and escaped-defect evidence under AGENTS.md when the move changes those behaviors. A mechanical move does not require unrelated runtime behavior or new tests. Check the implementation before accepting tool-noise evidence.

Remain read-only. Do not run wrappers, formatters, generators, staging, or destructive commands. Do not request blanket reruns when supplied proof covers the boundary.

Report findings with file evidence, a violated criterion or invariant, a reachable scenario, material impact, and a minimal correction. Classify each finding through AGENTS.md. Report pre-existing hardening only when relevant to the decision or explicitly requested. Flag scope escape and missing required proof. State manifest completeness, residual assumptions, and one verdict: `Approve`, `Approve with comments`, or `Request changes`.

Planning approval establishes that the bundle is ready for implementation. It does not replace required candidate review. Candidate approval requires the implemented diff to match the plan and supplied applicable green proof. Identify corrections that invalidate reviewed conclusions under AGENTS.md. Return findings for the orchestrator to collect before corrections.
