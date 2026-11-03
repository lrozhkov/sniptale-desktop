# Data handling

## Stored data

- Screenshot and recording temp media is protected, size-bounded, and owner-local. Incomplete writing state can expire under the transfer lifecycle policy. Finalized or rejected source media is retained through timeout, cancellation, failed publication, and browser rejection. Cleanup requires an authorized durable browser receipt with matching hash/byte count or verified local publication for the exact frozen job. Existing published files are rehashed before crash recovery can release their source.

  An explicit Desktop recovery action can resume at most 16 rejected items of each media kind per invocation. It preserves the frozen route, browser assignment, protected key, hash, and chunk progress. Browser reconnect never resumes rejection by itself.

- Manual profile and asset-job stores may contain destination paths required for recovery. They remain private runtime state and never enter diagnostics, IPC, or tray notices.
- Manual publication writes plaintext PNG/MP4 only below the validated user-selected destination. Published files are not removed by cache cleanup or uninstall.
- The private browser destination registry, installation pin identity, and explicit-Quit browser-startup guard are bounded owner state. Browser registration is not target selection. The per-install pipe secret uses current-user protection and is never logged. Per-media keys remain transfer-owned and are wiped when raw buffers are released.
- Structured logs rotate at 10 MiB, retain at most 10 managed files, and expire after 14 days. Support bundles contain selected redacted JSON entries, not raw logs or media.

Optional recording telemetry is projected into the captured frame and bounded by the finalized encoder duration. Missing or mismatched frame geometry and invalid optional snapshots omit telemetry while preserving the valid finalized video. Retained telemetry still passes the strict recovery validator before browser delivery.

## Forbidden output

Logs, doctor output, support bundles, operation failures, observability, and telemetry must exclude these values:

- Screenshot pixels, media bytes, and base64 chunks.
- Exclude typed text and clipboard values.
- URLs, cookies, auth headers, and tokens.
- Exclude raw Native Messaging payloads.
- Extension IDs/origins and controller or connection identifiers.
- Media, job, and destination identifiers.
- Local profile payloads and full local paths.

Allowed output contains only typed status/error categories, bounded counts and durations, source/geometry categories, and codec and audio capability facts. Byte counts, hashes, transfer offsets, install-health states, and explicit redaction labels are also allowed.

## Runtime directory trust

Windows diagnostics classify broad read or write access by Everyone, Authenticated Users, or BUILTIN\Users. Broad access to temp media fails closed. Logs, diagnostics, and cache report repair-required categories without exposing their paths.

## Deterministic proof

`contracts/security/redaction-corpus.json`, diagnostics and telemetry tests, `tooling/qa/rules/verify_security.py`, coverage, Windows smoke, and desktop E2E evidence enforce these rules. [Threat model](threat-model.md) owns residual disclosure risks.
