# ADR 0002: DPAPI owner-local adapters

Status: Accepted, 2026-07-09.

## Decision

Raw DPAPI mechanics exist only in `src/install/windows/dpapi.rs` and `src/transfer/windows_dpapi.rs`. Install is unwrap-only for the current-user pipe secret. Transfer wraps and unwraps per-media encryption keys. A shared crypto helper or third adapter requires a new architecture and security decision.

Both adapters reject empty and oversized input, reject null or empty output, keep `CRYPT_INTEGER_BLOB` bounds explicit, own `LocalAlloc` output with RAII, wipe secret output before `LocalFree`, return typed owner-local errors, and log no secret or path data.

## Trade-off

The small raw adapter shape is duplicated to preserve separate install and media-key authorities. Duplication can drift, so executable guardrails compare both implementations and the exact unsafe-adapter catalog.

## Proof

`tooling/qa/rules/verify_security.py` restricts raw symbols and invariants. `tooling/configs/qa/unsafe-ffi-adapter-proof.json` records safe wrappers, RAII owners, shutdown proof, and required reviews. Owner-local tests cover pipe-secret and protected-media behavior.
