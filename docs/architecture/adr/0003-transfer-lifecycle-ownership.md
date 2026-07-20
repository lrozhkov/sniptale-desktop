# ADR 0003: Transfer lifecycle ownership

Status: Accepted, 2026-07-09.

## Decision

`transfer/**` owns chunk sizing, base64 encoding, JSON budgets, hashes, protected reads, transfer identity, and lifecycle progression through writing, finalized, acknowledged, rejected, recovery, and cleanup states. Capture and recording keep their domain stores and metadata but consume transfer-owned policy such as `TransferLifecycleSnapshot`.

Native Messaging and the resident pipe remain command/control transports. Browser-facing media delivery is an explicit request/response path owned by transfer; media bytes never enter resident-pipe responses or duplicate-command caches.

## Trade-off

Central transfer policy prevents screenshot and recording recovery drift, while domain stores remain separate because they have different metadata and retention decisions. The boundary adds typed adapters between owners but keeps base64, integrity, and cleanup reviewable in one place.

## Proof

`src/transfer/lifecycle/mod.rs`, `src/transfer/chunks.rs`, and `src/transfer/protected_read.rs` own shared behavior. Capture/recording integration tests, `tooling/qa/rules/verify_architecture.py`, and `tooling/qa/rules/verify_security.py` prove the boundary.
