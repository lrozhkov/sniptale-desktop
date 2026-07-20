# Data handling

## Stored data

- Screenshot and recording temp media is protected, bounded, owner-local, and deleted after acknowledgement, explicit rejection, or retention expiry. Finalized-only recovery is allowed.
- Manual profile and asset-job stores may contain destination paths required for recovery. They remain private runtime state and never enter diagnostics, IPC, or tray notices.
- Manual publication writes plaintext PNG/MP4 only below the validated user-selected destination. Published files are not removed by cache cleanup or uninstall.
- The per-install pipe secret uses current-user protection and is never logged. Per-media keys remain transfer-owned and are wiped when raw buffers are released.
- Structured logs rotate at 10 MiB, retain at most 10 managed files, and expire after 14 days. Support bundles contain selected redacted JSON entries, not raw logs or media.

## Forbidden output

Logs, doctor output, support bundles, operation failures, observability, and telemetry must not contain screenshot pixels, media bytes, base64 chunks, typed text, clipboard values, URLs, cookies, auth headers, tokens, raw Native Messaging payloads, extension IDs/origins, controller or connection identifiers, media/job/destination identifiers, local profile payloads, or full local paths.

Allowed output is limited to typed status/error categories, bounded counts and durations, source/geometry categories, codec and audio capability facts, byte counts and hashes, transfer offsets, install-health states, and explicit redaction labels.

## Runtime directory trust

Windows diagnostics classify broad read or write access by Everyone, Authenticated Users, or BUILTIN\Users. Broad access to temp media fails closed; logs, diagnostics, and cache report repair-required categories without exposing their paths.

## Deterministic proof

`contracts/security/redaction-corpus.json`, diagnostics and telemetry tests, `tooling/qa/rules/verify_security.py`, coverage, Windows smoke, and desktop E2E evidence enforce these rules. [Threat model](threat-model.md) owns residual disclosure risks.
