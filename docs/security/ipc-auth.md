# IPC authorization

The browser-launched native host and resident tray process use a current-user named pipe for bounded control messages. Media bytes and base64 chunks never traverse this pipe.

## Authentication and consent

The first resident launch provisions a missing DPAPI current-user-protected per-install secret with `pipe-auth-v2-hmac-sha256`, using 256 bits from the OS random source. The install owner atomically publishes only complete ciphertext without overwriting an existing record. Concurrent launches converge on one credential. Bridge and diagnostic reads do not provision state. Before forwarding any browser payload, the bridge and resident complete a versioned HMAC-SHA-256 challenge. It covers the algorithm label, install ID, session ID, client/server nonces, and expiry.

The bridge supplies the already validated Native Messaging extension origin. Unknown authenticated connections require native user consent. Allow persists the origin, installed native-host executable hash, and a redacted parent fingerprint. Deny, timeout, unavailable UI, corrupt trust state, origin mismatch, wrong secret, stale challenge, or unsupported algorithm fails closed.

Parent-process facts are diagnostic provenance and prompt context, not an authorization allowlist.

## Session authorization

The server bounds concurrent sessions, frame sizes, pending challenges, replay memory, queues, and waits. Backpressure returns typed failure. Protocol v2 requires connection intent and compatible Desktop-settings/durable-ACK capabilities. Native settings and capture controls remain Desktop-owned.

A trusted observer may register a browser/profile target but cannot select it, mutate settings, or start/control capture. Only explicit Desktop selection grants a matching target lease. Stale, retired, observer, replayed, and wrong-target transfer requests fail closed. Media assignment and verified durable receipts govern source cleanup, not registration or heartbeat.

Pending announcements are batched and forget acknowledged IDs. A new lease resets the announcement projection.

A durable recording or history ACK for an absent pending transfer causes no change. It does not recreate media or claim new publication. Pending sources still require matching receipts and the selected target. Stale leases and storage failures remain errors.

Hello, trust, and heartbeat waits are bounded independently. A heartbeat does not extend the trust-consent deadline. Explicit Quit persists a startup guard before notifying live browser connections, and automatic reconnect cannot bypass it. User-requested reconnect and manual resident launch remain explicit startup actions.

Corrupt, oversized, wrong-user, wrong-install, unsupported-algorithm, or unprotectable credentials fail closed and are never silently replaced by startup. Explicit uninstall and clean reinstall remove authorization state and permit a fresh credential. Previous sessions cannot authenticate against it.

## Proof and residual risk

IPC owner tests cover valid/invalid challenges, replay, expiry, origin, consent, session limits, backpressure, disconnect, duplicate commands, and controller lease state. `verify_security.py`, Windows smoke, and desktop E2E prove the live installed path. A fully compromised current-user session remains outside this boundary. See the [threat model](threat-model.md).
