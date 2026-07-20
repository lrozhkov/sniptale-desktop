# IPC authorization

The browser-launched native host and resident tray process use a current-user named pipe for bounded control messages. Media bytes and base64 chunks never traverse this pipe.

## Authentication and consent

The installer provisions a DPAPI current-user-protected per-install secret with `pipe-auth-v2-hmac-sha256`. The bridge and resident perform a versioned HMAC-SHA-256 challenge over the algorithm label, install ID, session ID, client/server nonces, and expiry before forwarding any browser payload.

The bridge supplies the already validated Native Messaging extension origin. Unknown authenticated connections require native user consent. Allow persists the origin, installed native-host executable hash, and a redacted parent fingerprint; deny, timeout, unavailable UI, corrupt trust state, origin mismatch, wrong secret, stale challenge, or unsupported algorithm fails closed.

Parent-process facts are diagnostic provenance and prompt context, not an authorization allowlist.

## Session authorization

The server bounds concurrent sessions, frame sizes, pending challenges, replay memory, queues, and waits. Backpressure returns typed failure. Every operational command after handshake requires the active controller lease and current mode epoch; stale, replayed, duplicate, observer, and wrong-owner commands cannot start a second operation.

The pipe secret rotates after corruption, install identity/channel/user binding change, algorithm change, or tamper detection. Rotation invalidates existing sessions.

## Proof and residual risk

IPC owner tests cover valid/invalid challenges, replay, expiry, origin, consent, session limits, backpressure, disconnect, duplicate commands, and controller lease state. `verify_security.py`, Windows smoke, and desktop E2E prove the live installed path. A fully compromised current-user session remains outside this boundary; see the [threat model](threat-model.md).
