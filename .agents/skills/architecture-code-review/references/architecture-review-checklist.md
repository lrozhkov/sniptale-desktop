# Architecture review checklist

## Ownership

- Product behavior stays in the owner named by `docs/architecture/module-ownership.md`; root facades contain topology and explicit re-exports only.
- No god file, service bag, shared-helper authority, or new top-level owner appears without a contract update.
- App orchestration delegates parsing, validation, transfer, settings, diagnostics, and OS calls to their owners.

## Contracts and state

- Public and wire contracts stay platform-neutral; untrusted inputs are narrowed by owner parsers before use.
- Contract changes update registered fixtures, negative cases, tests, and all consumers.
- Authoritative, cached, advisory, and disposable state remain distinct; failed writes and partial operations have cleanup.
- State machines reject stale, duplicate, replayed, and wrong-owner events and make terminal behavior idempotent.

## Runtime

- Threads, channels, callbacks, handles, hooks, sessions, files, encoders, and streams have explicit bounded lifecycle owners.
- Production paths remain compiled and reachable; test-only moves or dead-code suppressions do not hide an orphaned Windows path.
- Unsafe work follows `docs/engineering/unsafe-ffi-policy.md`; blanket lint suppression requires a narrow cataloged exception and removal condition.

## Proof

- New owner behavior has deterministic owner-local proof and negative boundary cases.
- High-risk owners use coverage; dependency changes use supply-chain and license proof; Windows-only behavior has Windows-target or installed-runtime evidence.
- Repeated review findings become deterministic checks when feasible.
