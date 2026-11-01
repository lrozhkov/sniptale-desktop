---
name: repo-audit
description: Audit a bounded Sniptale Desktop repository scope without edits. Compare source, contracts, canonical tooling, documentation, and supplied QA evidence.
---

# Repository audit

Use this skill for an explicitly requested bounded repository audit. Receive the manifest, acceptance criteria, supplied proof, known exclusions, and repository state.

Read AGENTS.md, the documentation index, owner map, implementation rules, testing policy, and relevant security documents. Inspect source, contracts, catalogs, wrappers, tests, and documentation within the manifest. Use Git history when historical evidence matters.

Compare documented requirements with executable checks and supplied results. Identify missing coverage without inventing stronger product guarantees. Require runtime E2E only when the changed boundary or accepted criteria require it.

Remain read-only. Do not run QA wrappers, detectors, formatters, generators, staging, or destructive commands. Do not request blanket reruns or additional receipt hashes when supplied relevant proof is green.

Classify findings through AGENTS.md. Each blocker needs file evidence, a reachable scenario, the violated criterion, and a minimal correction. Keep unrelated defects and pre-existing hardening as recommendations. Flag scope escape before proposing new contracts or changes across additional owners.

Report the verdict first. Then report blockers, recommendations, inspected evidence, residual assumptions, and unverified areas. Use `Approve`, `Approve with comments`, or `Request changes`.
