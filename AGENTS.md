# AGENTS.md

Repository workflow contract for Sniptale Desktop.

## Scope and investigation

Define bounded acceptance criteria before implementation. Separate established defects, hypotheses, and implementation choices.

Before each diagnostic check, state the question, the smallest useful check, and the result that ends the investigation. Stop when evidence answers that question. Repeat a check only after relevant changes, a concrete failure, or an explicit workflow requirement.

Use the actual execution context for the first targeted behavioral proof. Internal state alone does not prove a browser, Windows, or installed-package outcome. Add broader proof only when the changed boundary requires it.

Preserve valid data and completed work when operations fail. Define failure behavior before implementation. Do not expand a bounded correction into unrelated hardening.

Analysis without tracked changes requires no QA wrappers. Local ignored evidence is not an implementation change. Before implementation, identify the owners, contracts, consumers, failure behavior, and required proof.

## Preflight

Before non-trivial work, read `docs/README.md`, the owner map, implementation rules, testing policy, and the security document for the touched boundary. Read the unsafe FFI policy before Windows adapter or raw-handle work. Tracked code, contracts, catalogs, and docs are authoritative. `.tmp/**` and `.artifacts/**` are evidence or local state, never source of truth.

Run `tooling/qa/wrappers/preflight.sh` when scope is unclear or before opening an owner seam.

## Implementation

- Keep product behavior in the owner named by `docs/architecture/module-ownership.md`. Keep facades thin and re-exports explicit.
- Treat JSON, IPC, storage, process output, registry data, and files as untrusted until owner parsing and validation succeed.
- Keep wire contracts platform-neutral, IPC framing in `ipc/**`, media chunking in `transfer/**`, and privileged Windows APIs in allowlisted owner-local adapters.
- Do not add native-owned accounts, cloud sync, gallery, editor, OCR, transcript, or AI behavior. ADR 0005 is limited to Manual settings and user-selected PNG/MP4 publication.
- Do not log media, typed text, clipboard values, URLs, tokens, raw payloads, identifiers, or full paths when a category is sufficient.
- Add owner-local deterministic proof for new behavior. Extend fixtures, dev dependencies, adapters, or wrappers when existing proof is insufficient.

Exact rules and their deterministic checks are owned by `docs/engineering/implementation-rules.md`. Judgment-only constraints are isolated there and require explicit review.

## Review findings

Classify each finding by the first applicable category:

1. `security issue`: a reachable violation of a documented security invariant.
2. `current regression`: the candidate breaks previously working behavior.
3. `acceptance blocker`: the candidate fails an explicit acceptance criterion or required repository gate.
4. `pre-existing hardening`: an existing weakness or optional improvement outside the requested acceptance.

Only the first three categories can block acceptance. Supply file evidence, a concrete failure scenario, and the smallest correction for each blocker. Label uncertainty and unverified areas explicitly. Do not turn unrelated recommendations into new acceptance criteria.

Use the architecture and security skills for their existing risk boundaries. Use `topology-plan-review` for large owner moves. Use `repo-audit` for an explicitly requested bounded repository audit. Reviewers inspect supplied proof and do not repeat wrappers without a specific evidence defect.

## Text and legal files

- Apply the repository `asd-ste100` skill to all English documentation, including this file, README, ADRs, procedures, and skills. Preserve requirement strength, conditions, exceptions, and technical names. The skill supports clear technical English without certifying dictionary compliance.
- Write each Markdown prose paragraph and each list item on one physical line. Do not hard-wrap prose to a column limit and do not run formatters that reflow Markdown.
- Preserve deliberate line structure in code blocks and machine formats.
- Copy third-party license text byte-for-byte from its authoritative source before adding project-specific copyright or provenance metadata. Do not reflow or paraphrase license text.
- Keep one document owner for each meaning. Link to the owner instead of copying rules, procedures, inventories, or thresholds.
- Update `tooling/configs/qa/docs.json` when tracked Markdown topology changes. `verify_docs.py` enforces topology, index coverage, links, ignore boundaries, and prose line policy.

## QA

Use wrappers instead of substitute ad hoc commands. The normal implementation flow is a coherent change, `release-harness.sh` when tooling/CI/docs-policy/skills changed, `checkpoint.sh`, required reviews, then `closeout.sh -m "message"`. `closeout.sh` stages and commits. Use `--no-commit` only for diagnostics or an explicit no-commit request.

Run `coverage.sh` for parser, IPC, transfer, settings, diagnostics, state-machine, or public contract changes. Dependency or license-policy changes require `licenses.sh --write` after review and `supply-chain.sh`. Normal gates use the non-mutating `licenses.sh --check`.

Run `audit.sh` from a complete Git checkout before public release. It requires the pinned Gitleaks version and scans both the worktree and full history. Findings and shallow history fail closed.

Run architecture and security review skills for privileged APIs, IPC authorization, diagnostics, telemetry, transfer, parser contracts, installer state, release provenance, or broad topology changes.

## Windows evidence

Run the Windows smoke lane for Windows runtime IPC, Native Messaging startup, installer/MSI, manifest registration, pipe authorization, autostart, or installed executable provenance:

```bash
tooling/qa/wrappers/windows-smoke-from-wsl.sh \
  --channel stable \
  --extension-origin chrome-extension://pcegkgbagkiagakmfjchgfgheepkllda/ \
  --dev-local-trust
```

Use `--skip-checkpoint` only after a fresh checkpoint. The WSL wrapper requires working PowerShell interop. Otherwise run `tooling/release/windows/smoke/windows-smoke.ps1` directly from Windows PowerShell.
