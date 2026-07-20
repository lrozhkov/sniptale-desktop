# Implementation rules

Each enforced rule names its machine authority. A rule without deterministic enforcement appears only under **Judgment-only rules**.

## Deterministically enforced

- `R-OWNER` — Product code stays in the owner defined by [module ownership](../architecture/module-ownership.md); new top-level owners, forbidden imports, god files, god directories, and broad facades fail `verify_architecture.py`, `verify_boundaries.py`, or `verify_topology.py`.
- `R-FACADE` — Root `mod.rs` and compatibility facades contain declarations, explicit re-exports, and compatibility aliases only; wildcard re-exports, facade chains, logic, IO, parsing, state mutation, Windows calls, and logging setup fail `verify_boundaries.py`.
- `R-BOUNDARY` — JSON, IPC, cache, files, registry values, process output, and command-line input remain untrusted until an owner parser narrows them; forbidden parsing sites and missing negative proof fail `verify_security.py`, `verify_contracts.py`, or `verify_test_policy.py`.
- `R-PROTOCOL` — `protocol/**` stays platform-neutral and every tracked Contract ID retains positive and negative fixtures; checked by `verify_architecture.py` and `verify_contracts.py`.
- `R-MEDIA` — Native Messaging and the resident pipe are control transports; base64, chunk bounds, hashes, protected reads, and media-transfer state stay in `transfer/**`; checked by architecture and security guardrails.
- `R-UNSAFE` — Unsafe is denied except exact owner-local Windows adapters present in `tooling/configs/qa/unsafe-ffi-adapter-proof.json`; checked by `verify_security.py` and the exception registry.
- `R-FAILURE` — Production code does not use `unwrap`, `expect`, `panic`, `todo`, `unimplemented`, stdout/stderr debug prints, or silent critical failures; enforced by Cargo lints and `verify_security.py`.
- `R-PRIVACY` — Logs and diagnostics exclude media, typed text, clipboard data, URLs, tokens, raw payloads, identifiers, and full paths; enforced by the redaction corpus, owner tests, and `verify_security.py`.
- `R-PROOF` — New non-scaffold owner behavior has owner-local deterministic proof; parser, IPC, transfer, settings, diagnostics, state-machine, and public-contract changes also run coverage; enforced by `verify_test_policy.py`, `verify_dev_dependencies.py`, and wrapper scope.
- `R-DEPS` — Dependency advisories, sources, bans, license expressions, and redistributed license text pass `cargo-audit`, `cargo-deny`, and `cargo-about` through the supply-chain and license wrappers.
- `R-SECRETS` — The current tree and complete Git history pass the pinned Gitleaks policy with redacted reports and a reviewed exact baseline; enforced locally by `audit.sh` and `release.sh`.
- `R-DOCS` — Authored Markdown, `LICENSE`, and `NOTICE` use one physical line per paragraph or list item; active documents are indexed once, local links resolve, and ignored local files are not authority; enforced by `verify_docs.py`.

## Judgment-only rules — review required

- Do not expand native product scope into account, cloud, gallery, editor, OCR, transcript, or AI behavior without an explicit product and architecture decision.
- Prefer owner-local adapters and typed errors over shared helper layers even when no import rule is violated.
- Split a role before it acquires independent state authority, lifecycle, parsing, IO, and presentation reasons to change.
- Document only information that code or machine policy cannot explain, and keep one document owner for each meaning.
- Reject a security or UX downgrade even when all deterministic gates pass. Record the accepted trade-off in an ADR or the threat model before implementation.
