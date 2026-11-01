---
name: topology-plan-review
description: Review plans and completed candidates for large Rust owner moves. Check consumers, collisions, contracts, and proof without edits.
---

# Topology plan review

Use this skill for a large owner move or a rename-heavy boundary change. Choose planning, candidate review, or wrapper diagnosis.

Read AGENTS.md, the module ownership map, implementation rules, and testing policy. Receive a complete bounded manifest with old and new paths. Require owner decisions, consumers, contract effects, collision handling, rollback, and a proof map.

In planning mode, identify predictable omissions before implementation. No green checkpoint is required for a plan. In candidate mode, compare the complete diff with the plan and supplied applicable green proof. In diagnosis mode, inspect supplied output without executing the wrapper.

Check every moved consumer, explicit re-export, fixture, source-reading check, catalog, build input, and documentation link. Decompose by independent responsibility, state, lifecycle, and reasons to change. Reject extra layers created only to satisfy counters. Do not create owners for hypothetical future features.

Remain read-only. Do not run wrappers, formatters, generators, staging, or destructive commands. Do not request blanket reruns when supplied proof covers the boundary.

Report findings with file evidence, a concrete scenario, and a minimal correction. Classify each finding through AGENTS.md. State manifest completeness, residual assumptions, and one verdict: `Approve`, `Approve with comments`, or `Request changes`.
