# Security review checklist

## Boundaries and authority

- Native Messaging, named-pipe, file, registry, process, and persisted inputs are bounded and parsed before use.
- Pipe clients complete the current challenge protocol; origin, consent, replay, expiry, session limits, controller lease, and mode epoch fail closed.
- Channel, host name, origin, install ID, executable path, signatures, manifests, autostart, and ACLs come from the install authority.

## Data and privileged resources

- Media, secrets, typed text, clipboard values, URLs, tokens, identifiers, payloads, and paths do not enter logs, diagnostics, telemetry, errors, evidence, or legal reports.
- Temporary media and keys remain owner-local, protected, bounded, wiped where required, and deleted on acknowledgement, rejection, expiry, or failed operation.
- Raw handles, allocations, hooks, callbacks, COM/MF/WASAPI objects, and DPAPI blobs have reviewed owner-local RAII and shutdown proof.

## Transfer, install, and release

- Chunks, JSON budgets, indexes, hashes, lifecycle transitions, protected reads, resume, and cleanup are verified before media release.
- Production EXE/MSI signatures, timestamps, revocation status, manifest hashes, legal payload, SBOM, audit reports, provenance, and distributable hashes are bound in release evidence.
- Dependency and license changes preserve the allowed-license policy, tracked exact texts, notice freshness, and runtime redistribution closure.
- Public release evidence includes a pinned, redacted secret scan of the current tree and complete Git history; shallow or skipped history is not acceptance evidence.

## Proof

- Negative tests cover malformed, oversized, stale, replayed, duplicate, wrong-owner, wrong-origin, corrupted, and cleanup paths.
- Redaction corpus, Windows smoke, desktop E2E, coverage, supply-chain, and license evidence are present where required.
- A repeatable security finding is proposed as a deterministic guardrail when feasible.
