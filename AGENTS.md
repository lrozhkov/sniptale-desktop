# AGENTS.md

Repository workflow contract for Sniptale Desktop.

## Preflight

Before non-trivial work, read `docs/README.md`, the owner map, implementation rules, testing policy, and the security document for the touched boundary. Read the unsafe FFI policy before Windows adapter or raw-handle work. Tracked code, contracts, catalogs, and docs are authoritative; `.tmp/**` and `.artifacts/**` are evidence or local state, never source of truth.

Run `tooling/qa/wrappers/preflight.sh` when scope is unclear or before opening an owner seam.

## Implementation

- Keep product behavior in the owner named by `docs/architecture/module-ownership.md`; keep facades thin and re-exports explicit.
- Treat JSON, IPC, storage, process output, registry data, and files as untrusted until owner parsing and validation succeed.
- Keep wire contracts platform-neutral, IPC framing in `ipc/**`, media chunking in `transfer/**`, and privileged Windows APIs in allowlisted owner-local adapters.
- Do not add native-owned accounts, cloud sync, gallery, editor, OCR, transcript, or AI behavior. ADR 0005 is limited to Manual settings and user-selected PNG/MP4 publication.
- Do not log media, typed text, clipboard values, URLs, tokens, raw payloads, identifiers, or full paths when a category is sufficient.
- Add owner-local deterministic proof for new behavior. Extend fixtures, dev dependencies, adapters, or wrappers when existing proof is insufficient.

Exact rules and their deterministic checks are owned by `docs/engineering/implementation-rules.md`. Judgment-only constraints are isolated there and require explicit review.

## Text and legal files

- Write each Markdown prose paragraph and each list item on one physical line. Do not hard-wrap prose to a column limit and do not run formatters that reflow Markdown.
- Preserve deliberate line structure in code blocks and machine formats.
- Copy third-party license text byte-for-byte from its authoritative source before adding project-specific copyright or provenance metadata. Do not reflow or paraphrase license text.
- Keep one document owner for each meaning; link to the owner instead of copying rules, procedures, inventories, or thresholds.
- Update `tooling/configs/qa/docs.json` when tracked Markdown topology changes. `verify_docs.py` enforces topology, index coverage, links, ignore boundaries, and prose line policy.

## QA

Use wrappers instead of substitute ad hoc commands. The normal implementation flow is a coherent change, `release-harness.sh` when tooling/CI/docs-policy/skills changed, `checkpoint.sh`, required reviews, then `closeout.sh -m "message"`. `closeout.sh` stages and commits; use `--no-commit` only for diagnostics or an explicit no-commit request.

Run `coverage.sh` for parser, IPC, transfer, settings, diagnostics, state-machine, or public contract changes. Dependency or license-policy changes require `licenses.sh --write` after review and `supply-chain.sh`; normal gates use the non-mutating `licenses.sh --check`.

Run `audit.sh` from a complete Git checkout before public release. It requires the pinned Gitleaks version and scans both the worktree and full history; findings and shallow history fail closed.

Run architecture and security review skills for privileged APIs, IPC authorization, diagnostics, telemetry, transfer, parser contracts, installer state, release provenance, or broad topology changes.

## Windows evidence

Run the Windows smoke lane for Windows runtime IPC, Native Messaging startup, installer/MSI, manifest registration, pipe authorization, autostart, or installed executable provenance:

```bash
tooling/qa/wrappers/windows-smoke-from-wsl.sh \
  --channel stable \
  --extension-origin chrome-extension://pcegkgbagkiagakmfjchgfgheepkllda/ \
  --dev-local-trust
```

Use `--skip-checkpoint` only after a fresh checkpoint. The WSL wrapper requires working PowerShell interop; otherwise run `tooling/release/windows/smoke/windows-smoke.ps1` directly from Windows PowerShell.
