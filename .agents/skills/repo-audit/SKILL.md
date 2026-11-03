---
name: repo-audit
description: Audit a bounded Sniptale Desktop repository scope without edits. Compare source, contracts, canonical tooling, documentation, and supplied QA evidence.
---

# Repository audit

Use this skill for an explicitly requested bounded repository audit. Use a fresh independent read-only auditor with `fork_turns: "none"`. Do not reuse an agent that implemented the audited changes. Receive the scope manifest, exact repository or diff scope, preflight results, supplied proof, known failures, exclusions, and repository state. Do not receive intended conclusions.

For final repository acceptance, audit after supplied required proof and focused reviews. Do not impose this final-acceptance sequence on a diagnostic audit.

Read AGENTS.md, the documentation index, owner map, implementation rules, testing policy, and relevant security documents. Inspect source, contracts, catalogs, wrappers, tests, and documentation within the manifest. Use Git history when historical evidence matters.

Compare documented requirements with executable checks and supplied results. Identify missing coverage without inventing stronger product guarantees. Require runtime E2E only when the changed boundary or accepted criteria require it.

Check persistence concurrency case selection and test execution when persistence is in scope. Check escaped-defect proof, why QA missed the defect, and the deterministic-guard decision when applicable. Apply AGENTS.md to exact tool-noise and debt records. A maintenance report does not replace acceptance proof. Do not launch structural detectors or invent another control inventory.

Remain read-only. Do not run QA wrappers, detectors, formatters, generators, staging, or destructive commands. Do not request blanket reruns or additional receipt hashes when supplied relevant proof is green.

Classify findings through AGENTS.md. Each blocker needs file evidence, a reachable scenario, a violated criterion or invariant, material impact, and a minimal correction. Label inference explicitly.

Report pre-existing hardening only when it affects a task decision or the user requests it. Flag scope escape before proposing new contracts or changes across additional owners. Report missing required checks without waiving them. Identify corrections that invalidate reviewed conclusions. Return findings for collection before the correction batch.

Report the verdict first. Then report blockers, recommendations, inspected evidence, residual assumptions, and unverified areas. Use `Approve`, `Approve with comments`, or `Request changes`.
