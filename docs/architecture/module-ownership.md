# Module ownership

The stable product owner set is `app`, `protocol`, `ipc`, `settings`, `assets`, `tray`, `hotkeys`, `capture`, `recording`, `audio`, `telemetry`, `transfer`, `install`, and `diagnostics`. A new top-level `src/**` owner requires an architecture decision and guardrail update.

## Owners

- `app/**` owns process lifecycle, startup and shutdown order, bounded worker coordination, and mode orchestration. It does not own parsing, persistence policy, media conversion, redaction, or Windows API adapters.
- `protocol/**` owns platform-neutral wire DTOs, versions, parser narrowing, and typed protocol errors.
- `ipc/**` owns Native Messaging and named-pipe framing plus pipe authentication mechanics. These are control transports, not media buses.
- `settings/**` owns operating mode, mode epoch, accepted extension snapshots, Manual profile, effective settings, and UI appearance preference. Appearance is not capture authority.
- `assets/**` owns durable Manual publication jobs, destination validation, atomic PNG/MP4 publication, and recovery. It is not a library, editor, or cloud owner.
- `tray/**` owns shell presentation, native consent, menu input, notices, and the settings-window adapter. It projects typed state and never validates or persists settings.
- `hotkeys/**` owns global hotkey registration and dispatch for both accepted extension actions and Manual actions.
- `capture/**` owns screenshot source resolution, region selection, Windows Graphics Capture, PNG encoding, geometry, and screenshot temp-store orchestration.
- `recording/**` owns recording state, encoder lifecycle, quality application, disk pressure, MP4 finalization, and recording temp state.
- `audio/**` owns endpoint discovery, microphone and loopback capture, PCM mixing, resampling, and drift handling.
- `telemetry/**` owns recording-only keyboard/mouse hooks and sanitization. It never emits typed text.
- `transfer/**` owns media-key protection, bounded reads, base64 chunks, hashes, browser-transfer progression, retention, and cleanup.
- `install/**` owns runtime install identity, manifests, autostart, signatures, ACL assessment, registration repair assessment, and pipe-secret provisioning.
- `diagnostics/**` owns redacted logs, health checks, observability summaries, crash categories, doctor output, and support bundles. It never receives raw media payloads.

## State authority

Authoritative state belongs only to the owner above. Advisory caches and disposable temp state must be separate types and paths. Read paths do not repair persisted state. Capture and recording persist an immutable authority/delivery snapshot before media side effects.

DPAPI is intentionally split: `install/**` unwraps the per-install pipe secret and `transfer/**` protects per-media keys. [ADR 0002](adr/0002-dpapi-owner-local-adapters.md) owns that exception.

## Dependency direction

`protocol/**` imports no product owner. `ipc/**`, `settings/**`, and `transfer/**` may consume protocol types but not OS-owner implementations. Media and OS owners do not import IPC transports. `diagnostics/**` consumes typed summaries, not media owners. Cross-owner calls use narrow typed interfaces coordinated by `app/**`.

Windows handles, COM objects, callbacks, and unsafe operations remain inside the owner-local adapter that acquires them. Platform-neutral state and wire types contain no registry paths, pipe names, HWND values, WGC concepts, or WASAPI identifiers.
