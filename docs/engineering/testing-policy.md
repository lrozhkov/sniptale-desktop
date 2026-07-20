# Testing policy

Tests prove owner behavior and trust-boundary failure modes. Exact test inventory, scenario size limits, property markers, performance markers, and coverage floors are machine-owned by `tooling/qa/rules/verify_test_policy.py` and `tooling/qa/rules/coverage-floor.py`.

## Contract tests

Every tracked wire Contract ID requires an accepted fixture and rejected malformed, incompatible, oversized, stale, or replayed variants as applicable. Executable fixtures live under `contracts/**` or the owning test module, never under ignored paths.

## Boundary parser tests

Native Messaging, named-pipe, persisted cache, file, registry, process-output, and command-line parsers require valid and invalid cases. Size rejection must occur before unbounded allocation or body reads.

## Property regression tests

Risky identifier parsers, framing, transfer progression, settings bounds, redaction, and authorization state use deterministic property tests when examples cannot cover the input space. Registered `proptest!` markers may be removed only with equivalent machine-registered proof.

## State-machine tests

Recording, screenshot, transfer, controller lease, settings mode, asset publication, and repair assessment cover success, cancel/reject, duplicate, stale, wrong owner, reconnect/resume, failure cleanup, and terminal idempotency.

## Transfer integrity tests

Transfer proof covers total bytes, raw and base64 chunk limits, complete JSON budget, indexes, per-chunk and whole-file SHA-256, protected read failures, finalized-only recovery, acknowledgement, rejection, expiry, and cleanup.

## Diagnostics redaction tests

The corpus at `contracts/security/redaction-corpus.json` is checked against logs, doctor output, support bundles, operation failures, telemetry surfaces, and recent-error summaries. Tests assert both sensitive-value absence and useful typed categories.

## Coverage floors

`coverage-floor.py` is the only owner of numeric aggregate and owner floors. Floors ratchet upward and are not copied into documentation. Coverage supplements negative proof and never replaces it.

## Platform proof

Deterministic owner tests run on Linux where possible. Windows-target check/test compilation is required in checkpoint. Privileged API, MSI, Native Messaging registration, tray, DPI, capture, recording, audio, consent, and accessibility behavior uses Windows smoke or isolated desktop E2E evidence when unit tests cannot prove the OS result.
