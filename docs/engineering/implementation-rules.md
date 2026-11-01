# Implementation rules

Each enforced rule names its machine authority. A rule without deterministic enforcement appears only under **Judgment-only rules**.

## Deterministically enforced

- `R-OWNER` — Product code stays in the owner defined by [module ownership](../architecture/module-ownership.md). New top-level owners, forbidden imports, broad facades fail `verify_architecture.py`, `verify_boundaries.py`, or `verify_topology.py`.
- `R-SIZE` — Rust functions use Clippy `too_many_lines` at its standard threshold of 100. Only Clippy counts function lines.

  Native probes use pinned Rust 1.99.1 / Clippy 0.1.99. The signature, enclosing body braces, and blank/comment-only lines do not count. A function with 100 body code lines passes. A function with 101 fails. This is Clippy behavior, not a second project line-count implementation. The standard threshold is documented in [Clippy too_many_lines](https://clippy.rust-lang.org/stable/index.html#too_many_lines).

  Ordinary Rust files warn above 500 physical lines and fail above 800. Comments and blank lines count. These file thresholds are project policy, not an industry standard. `main.rs` keeps its 80-line limit and root owner `mod.rs` keeps its 120-line limit.

  File counts only warn above 20 immediate Rust files (including `mod.rs`) or 60 recursive Rust files in each module directory. Counts include tests.

  `verify_boundaries.py` owns file and module checks. Checkpoint uses identical local and CI Cargo/Clippy settings. Checks cover host and Windows MSVC targets, including production and test code.

  Size warnings use a separate `WARN code-size:` channel and never change the exit code or become Rust warnings under `-D warnings`.

- `R-SIZE-SCOPE` — File-size checks cover `build.rs` and every tracked/source Rust file under `src/**`, including tests, generated source, and declarative tables. Build output outside `src/**` is not authored source.

  Existing large scenario files retain only the exact limits and per-file reasons registered through `tooling/configs/qa/code-size-exceptions.json` and the exception registry. New generated/table/test exceptions require an exact file, reason and ceiling. Missing files, limits no longer needed, duplicate scopes and exceeded ceilings fail.

  Function exceptions use function-local `#[expect(clippy::too_many_lines, reason = "...")]`. Broad module/file suppression and `#[allow]` fail, and Clippy rejects unfulfilled expectations under `-D warnings`. Clippy applies to compiled production and test functions via `--all-targets --all-features`. External macro expansions follow Clippy semantics without a duplicate source parser.

- `R-FACADE` — Root `mod.rs` and compatibility facades contain declarations, explicit re-exports, and compatibility aliases only. Wildcard re-exports, facade chains, logic, IO, parsing, state mutation, Windows calls, and logging setup fail `verify_boundaries.py`.
- `R-BOUNDARY` — JSON, IPC, cache, files, registry values, process output, and command-line input remain untrusted until an owner parser narrows them. Forbidden parsing sites and missing negative proof fail `verify_security.py`, `verify_contracts.py`, or `verify_test_policy.py`.
- `R-PROTOCOL` — `protocol/**` stays platform-neutral and every tracked Contract ID retains positive and negative fixtures. `verify_architecture.py` and `verify_contracts.py` enforce this rule.
- `R-MEDIA` — Native Messaging and the resident pipe are control transports. Base64, chunk bounds, hashes, protected reads, and media-transfer state stay in `transfer/**`. Architecture and security guardrails enforce this rule.
- `R-UNSAFE` — Unsafe is denied except exact owner-local Windows adapters present in `tooling/configs/qa/unsafe-ffi-adapter-proof.json`. `verify_security.py` and the exception registry enforce this rule.
- `R-FAILURE` — Production code does not use `unwrap`, `expect`, `panic`, `todo`, `unimplemented`, stdout/stderr debug prints, or silent critical failures. Cargo lints and `verify_security.py` enforce this rule.
- `R-PRIVACY` — Logs and diagnostics exclude media, typed text, clipboard data, URLs, tokens, raw payloads, identifiers, and full paths. The redaction corpus, owner tests, and `verify_security.py` enforce this rule.
- `R-PROOF` — New non-scaffold owner behavior has owner-local deterministic proof. Parser, IPC, transfer, settings, diagnostics, state-machine, and public-contract changes also run coverage. `verify_test_policy.py`, `verify_dev_dependencies.py`, and wrapper scope enforce this rule.
- `R-DEPS` — Dependency advisories, sources, bans, license expressions, and redistributed license text pass `cargo-audit`, `cargo-deny`, and `cargo-about` through the supply-chain and license wrappers.
- `R-SECRETS` — The current tree and complete Git history pass the pinned Gitleaks policy with redacted reports and a reviewed exact baseline. `audit.sh` and `release.sh` enforce this rule locally.
- `R-DOCS` — Authored Markdown, `LICENSE`, and `NOTICE` use one physical line per paragraph or list item. Active documents are indexed once, local links resolve, and ignored local files are not authority. `verify_docs.py` also enforces English structural rules through the repository asd-ste100 checker. Lexical and voice advice remains nonblocking. Exact legal and generator-owned text retains its original bytes.

## Judgment-only rules — review required

- Do not expand native product scope into account, cloud, gallery, editor, OCR, transcript, or AI behavior without an explicit product and architecture decision.
- Prefer owner-local adapters and typed errors over shared helper layers even when no import rule is violated.
- Decompose by independent responsibility, state authority, lifecycle, and reasons to change. Do not add functions, files, or directories solely to satisfy a counter. Do not mass-merge existing small files. Combine or split code while working on that area only when cohesion and readability improve.
- Document only information that code or machine policy cannot explain, and keep one document owner for each meaning.
- Reject a security or UX downgrade even when all deterministic gates pass. Record the accepted trade-off in an ADR or the threat model before implementation.
