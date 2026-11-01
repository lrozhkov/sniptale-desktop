# AGENTS.md

Repository workflow contract for Sniptale Desktop.

## Scope and investigation

Before implementation, record a scope manifest with observable acceptance criteria, non-goals, planned files, owners, contracts, and affected consumers. Include failure behavior, required proof, and review applicability. Keep local task evidence under ignored `.tmp/**`. It does not replace tracked policy.

Separate established defects, hypotheses, and implementation choices. Test a hypothesis before treating it as an architecture constraint. Start with the simplest approach that meets acceptance and existing invariants. Add infrastructure only when evidence shows a limitation. State a reason for each accuracy tolerance. Unknown accuracy does not imply zero error.

Before each diagnostic check, state the question, the smallest useful check, and the result that ends the investigation. Stop when evidence answers that question. Repeat a check only after relevant changes, a concrete failure, or an explicit workflow requirement.

Use the actual execution context for the first targeted behavioral proof. Internal state alone does not prove a browser, Windows, or installed-package outcome. Add broader proof only when the changed boundary requires it.

Preserve valid data and completed work when operations fail. Define failure behavior before implementation. Restrict only operations affected by the failure. Increase scope only for acceptance, an established defect, an existing invariant, a supported flow, or a material risk. Material risks concern security, privacy, authorization, trust boundaries, or permanent data loss.

Divide implementation into separately verifiable steps. Complete each step's proof before starting an independent step. Combine steps when a dependency requires it. Do not expand a bounded correction into unrelated hardening.

Analysis without tracked changes requires no implementation QA or closeout. Check changes to ignored local instructions directly. Do not run wrappers for unrelated tracked changes. For tracked changes, use the QA sequence below. An explicit no-commit request still requires applicable proof and review. Stop before closeout and report it as not run.

## Working directory and permissions

Use the current working directory unless the user selects another. Check Git state before editing. Preserve changes outside the task scope. Create a worktree only with explicit user permission. This restriction also applies to subagents. Implementation, testing, review, or commit authorization alone does not authorize another worktree.

Use permissions available in the current environment. Identify the cause of a permission failure before retrying. Use escalation only when the environment permits it and the diagnosed failure requires it.

## Preflight

Before non-trivial work, read `docs/README.md`, the owner map, implementation rules, testing policy, and the security document for the touched boundary. Read the unsafe FFI policy before Windows adapter or raw-handle work. Tracked code, contracts, catalogs, and docs are authoritative. `.tmp/**` and `.artifacts/**` are evidence or local state, never source of truth.

Run `tooling/qa/wrappers/preflight.sh` before changes to any of these areas:

- Production behavior or public contracts.
- Persistence, runtime boundaries, or permissions.
- Topology, QA tooling, workflow policy, review skills, or multiple owners.

Also run preflight when scope is unclear or before opening an owner seam. Record planned paths in the scope manifest before running it. Use its output and the owner map to identify documents, consumers, risks, proof, and required reviews.

This wrapper does not produce an automatic change-risk report. Inspect the actual implementation when selecting reviews. Checkpoint does not replace required preflight before implementation. Repeat preflight before entering additional scope.

## Broad moves

A broad move crosses an owner boundary or changes a public import path. Before implementation, inventory every affected file and consumer. Record old and new paths, ownership, contract effects, collisions, file modes, content digests, rollback, and required proof.

Use `topology-plan-review` before implementing a planned broad move. If prior review was missed, review the complete candidate after applicable green proof. Planning approval does not replace required candidate architecture or security review.

## Implementation

- Keep product behavior in the owner named by `docs/architecture/module-ownership.md`. Keep facades thin and re-exports explicit.
- Treat JSON, IPC, storage, process output, registry data, and files as untrusted until owner parsing and validation succeed.
- Keep wire contracts platform-neutral, IPC framing in `ipc/**`, media chunking in `transfer/**`, and privileged Windows APIs in allowlisted owner-local adapters.
- Do not add native-owned accounts, cloud sync, gallery, editor, OCR, transcript, or AI behavior. ADR 0005 is limited to Manual settings and user-selected PNG/MP4 publication.
- Do not log media, typed text, clipboard values, URLs, tokens, raw payloads, identifiers, or full paths when a category is sufficient.
- Add owner-local deterministic proof for new behavior. Extend fixtures, dev dependencies, adapters, or wrappers when existing proof is insufficient.
- For a defect missed by existing QA, add proof that fails before the fix. Record why QA missed it. Record whether a deterministic guard is needed.

Exact rules and their deterministic checks are owned by `docs/engineering/implementation-rules.md`. Judgment-only constraints are isolated there and require explicit review.

## Persistence concurrency proof

For persistence changes, record these cases in the scope manifest:

- Parallel writers.
- Delayed writes after deletion or restore.
- Recreation of the same identity.
- Command retry after failure or refusal.

For each applicable case, record the invariant, test file, and test name. For each inapplicable case, give a reason from the supported flow. Include startup, registration, recovery, and background operations that can write before an interactive session exists.

Use controlled ordering in concurrency tests. Do not use timing delays to create races. Reviewers check case selection, evidence, and reasons for inapplicability. Ensure checkpoint executes the selected tests through its existing test discovery and policy. Correct missed discovery and prove execution through checkpoint. Do not add a separate advisory race report or heuristic detector.

## Review findings

Classify each finding by the first applicable category:

1. `security issue`: evidence shows a reachable violation of an existing security invariant or a material security risk within scope.
2. `current regression`: the candidate introduces or worsens a defect that is not a security issue.
3. `acceptance blocker`: the candidate fails an explicit acceptance criterion or required repository gate.
4. `pre-existing hardening`: an existing weakness or optional improvement outside the requested acceptance.

Only the first three categories can block acceptance. Supply file evidence, the violated criterion or invariant, a reachable failure scenario, material impact, and the smallest correction. Label inference, uncertainty, and unverified areas explicitly. Report pre-existing hardening only when it affects a task decision or the user requests it. Do not turn unrelated recommendations into new acceptance criteria.

Check the actual diff against architecture and security review triggers. Missing automatic routing does not cancel an applicable review. Run both skills for the risk boundaries listed under QA. Record an implementation-based reason for each review that does not apply. Use `topology-plan-review` for broad moves. Use `repo-audit` for an explicitly requested bounded repository audit.

For each required review, spawn a fresh independent read-only agent with `fork_turns: "none"`. Supply the scope manifest, exact diff, preflight result, affected consumers, QA results, known failures, and exclusions. Do not reuse an implementation agent as a reviewer. Do not supply intended conclusions.

Run applicable architecture and security reviews after green checkpoint and applicable harness proof. They may run in parallel. Retain each verdict and its evidence under ignored `.tmp/qa/reviews/`. A self-review does not satisfy this stage.

Reviewers inspect supplied proof without running wrappers. The main agent runs all blocking QA wrappers. A reviewer must identify a specific evidence defect before requesting more proof. Require new behavior or proof only for a reachable trigger affecting acceptance or a material invariant. Repeat semantic review only when corrections change behavior, ownership, public contracts, dependency direction, parser semantics, security, or material topology. Mechanical cleanup alone does not require another semantic review.

Treat heuristic findings as investigation inputs. Inspect the flagged implementation first. Investigate a guardrail only when concrete evidence suggests a false positive. Prove tool-noise before requesting an exact registry entry.

Follow the central registry requirements in `docs/tooling/qa.md`. Temporary debt never waives a blocking rule. Do not weaken a rule or add a broad exception.

These are agent workflow stages. Shell wrappers do not perform semantic review or create a review verdict.

## Text and legal files

- Apply the repository `asd-ste100` skill to all English documentation, including this file, README, ADRs, procedures, and skills. Preserve requirement strength, conditions, exceptions, and technical names. The skill supports clear technical English without certifying dictionary compliance.
- Write each Markdown prose paragraph and each list item on one physical line. Do not hard-wrap prose to a column limit and do not run formatters that reflow Markdown.
- Preserve deliberate line structure in code blocks and machine formats.
- Copy third-party license text byte-for-byte from its authoritative source before adding project-specific copyright or provenance metadata. Do not reflow or paraphrase license text.
- Keep one document owner for each meaning. Link to the owner instead of copying rules, procedures, inventories, or thresholds.
- Update `tooling/configs/qa/docs.json` when tracked Markdown topology changes. `verify_docs.py` enforces topology, index coverage, links, ignore boundaries, and prose line policy.

## QA

Use wrappers instead of substitute ad hoc commands. Complete these stages in order for tracked changes:

1. Complete the coherent candidate and deterministic proof for required success and failure paths.
2. Run `tooling/qa/wrappers/release-harness.sh` when tooling, CI, docs policy, or repository skills changed.
3. Run `tooling/qa/wrappers/checkpoint.sh`.
4. Complete applicable independent reviews using the supplied green proof.
5. Collect all review findings before corrections. Check each finding against acceptance, evidence, and existing invariants.
6. Apply one correction batch. Repeat only proof and review invalidated by those corrections. Resolve blockers before closeout.
7. Run `tooling/qa/wrappers/closeout.sh -m "message"` unless the user requested no commit.

Closeout owns its required checkpoint runs, scoped coverage, supply-chain checks, build, diff validation, staging, and commit. Do not skip its internal stages. Use `--no-commit` only for diagnostics. An explicit no-commit request uses the proof and review flow described above.

The main agent must run blocking wrappers sequentially. Do not stage manually, amend a commit, stage task artifacts, or replace closeout unless the user requests an exception. Separate user-requested commits require separate closeout runs.

If closeout fails, diagnose only its failed stage. Correct the defect and rerun closeout. Return to preflight if the failure reveals incorrect ownership, topology, proof scope, or runtime boundaries.

Run `coverage.sh` for parser, IPC, transfer, settings, diagnostics, state-machine, or public contract changes. Dependency or license-policy changes require `licenses.sh --write` after review and `supply-chain.sh`. Normal gates use the non-mutating `licenses.sh --check`.

Run `static-analysis.sh` before public release through the canonical release wrapper. It runs pinned jscpd and CodeQL with the same settings locally and in CI. Use the central finding registry for reviewed tool-noise and temporary debt. Do not use broad baselines, count budgets, or inline analyzer suppressions.

Run `audit.sh` from a complete Git checkout before public release. It requires the pinned Gitleaks version and scans both the worktree and full history. Findings and shallow history fail closed.

Run architecture and security review skills for privileged APIs, IPC authorization, diagnostics, telemetry, transfer, parser contracts, installer state, release provenance, or broad topology changes.

## Unavailable checks and final report

If a required tool, reviewer, or check is unavailable, preserve completed work. Continue work that does not depend on the missing result. Report the exact blocker and remaining required action. Do not cancel the requirement or claim completion without its result.

Report the change, commit, required QA and closeout statuses, review results, and unresolved limits. Use `passed`, `failed`, or `not run` for each required command. Report inapplicable reviews with their implementation-based reasons. For a defect missed by QA, report its cause and the deterministic-guard decision. After the fourth closeout attempt, also report repeated causes and resulting policy or guardrail changes.

## Windows evidence

Run the Windows smoke lane for Windows runtime IPC, Native Messaging startup, installer/MSI, manifest registration, pipe authorization, autostart, or installed executable provenance:

```bash
tooling/qa/wrappers/windows-smoke-from-wsl.sh \
  --channel stable \
  --extension-origin chrome-extension://pcegkgbagkiagakmfjchgfgheepkllda/ \
  --dev-local-trust
```

Use `--skip-checkpoint` only after a fresh checkpoint. The WSL wrapper requires working PowerShell interop. Otherwise run `tooling/release/windows/smoke/windows-smoke.ps1` directly from Windows PowerShell.
