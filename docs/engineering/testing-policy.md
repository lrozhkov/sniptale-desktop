# Testing policy

Tests prove owner behavior and trust-boundary failure modes. Exact test inventory, property markers, performance markers, and coverage floors are machine-owned by `tooling/qa/rules/verify_test_policy.py` and `tooling/qa/rules/coverage-floor.py`.

File and function sizes, including the exact existing test-file exceptions, follow [implementation rules](implementation-rules.md). Native Clippy boundary probes and QA negative tests verify threshold edges, warning propagation, exact exceptions and stale expectations.

## Contract tests

Every tracked wire Contract ID requires an accepted fixture and rejected malformed, incompatible, oversized, stale, or replayed variants as applicable. Executable fixtures live under `contracts/**` or the owning test module, never under ignored paths.

## Boundary parser tests

Native Messaging, named-pipe, persisted cache, file, registry, process-output, and command-line parsers require valid and invalid cases. Size rejection must occur before unbounded allocation or body reads.

## Property regression tests

Risky identifier parsers, framing, transfer progression, settings bounds, redaction, and authorization state use deterministic property tests when examples cannot cover the input space. Registered `proptest!` markers may be removed only with equivalent machine-registered proof.

## State-machine tests

Recording, screenshot, transfer, controller lease, settings mode, asset publication, and repair assessment cover success, cancel/reject, duplicate, stale, wrong owner, reconnect/resume, failure cleanup, and terminal idempotency.

## Transfer integrity tests

Transfer proof covers total bytes, raw and base64 chunk limits, complete JSON budget, indexes, and per-chunk and whole-file SHA-256. It also covers protected read failures, finalized-only recovery, acknowledgement, rejection, expiry, and cleanup.

## Diagnostics redaction tests

The corpus at `contracts/security/redaction-corpus.json` is checked against logs, doctor output, support bundles, operation failures, telemetry surfaces, and recent-error summaries. Tests assert both sensitive-value absence and useful typed categories.

## Coverage floors

`coverage-floor.py` is the only owner of numeric aggregate and owner floors. Floors ratchet upward and are not copied into documentation. Coverage supplements negative proof and never replaces it.

Development and release scopes follow [QA tooling](../tooling/qa.md). Full release proof retains all integration scenarios. Development exclusions never replace coverage or release acceptance.

Nextest preserves the complete selected result matrix and failing-test output in the JUnit report configured by `.config/nextest.toml`. The bounded QA console log is not a complete test-result inventory.

## Platform proof

Deterministic owner tests run on Linux where possible. Rust scopes require Windows-target check/test compilation in checkpoint. Full checkpoints always require this proof. Use Windows smoke or isolated desktop E2E when unit tests cannot prove the OS result. This applies to privileged APIs, MSI, Native Messaging registration, tray, DPI, capture, recording, audio, consent, and accessibility.
