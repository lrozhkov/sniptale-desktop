# Unsafe and FFI policy

The crate denies unsafe code by default. A privileged Windows adapter may receive an exact exception only when a safe binding cannot express the required API boundary.

## Admission contract

An exception requires all of these conditions:

- An owner-local adapter.
- A safety invariant above every unsafe block.
- A typed safe wrapper.
- An RAII owner for every raw resource.
- Deterministic teardown/cancel proof or Windows integration evidence.
- Architecture and security reviews.
- Exact entries in both `tooling/configs/qa/exception-registry.json` and `tooling/configs/qa/unsafe-ffi-adapter-proof.json`.

Raw handles, COM pointers, callback state, hooks, windows, sessions, streams, and allocated blobs do not cross the owner boundary. Callback lifetimes and thread/apartment affinity are explicit. Cleanup covers success, cancel, failure, shutdown, and partial construction.

## Deterministic enforcement

`tooling/qa/rules/verify_security.py` derives allowed adapters from the two machine catalogs. It rejects missing or stale files and checks required proof fields. It scans for unregistered unsafe code and privileged symbols. `tooling/qa/rules/verify_boundaries.py` prevents broad suppressions and cross-owner handle flow.

DPAPI remains limited to the two adapters accepted by [ADR 0002](../architecture/adr/0002-dpapi-owner-local-adapters.md).

## Review-only attention

Machine catalogs prove registration and declared invariants, not the semantic correctness of an FFI lifetime. Reviewers must inspect pointer validity, ownership transfer, callback reentrancy, error-path cleanup, and Windows evidence for every changed adapter.
