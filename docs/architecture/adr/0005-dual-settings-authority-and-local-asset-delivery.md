# ADR 0005: Desktop settings authority and recoverable media delivery

Status: Accepted, 2026-07-11. Authority and delivery revised 2026-10-31 for the explicitly requested Desktop/extension integration.

## Decision

Desktop owns the local capture/recording profile, effective settings, hotkeys, and native controls. The persisted `OperatingMode` variants `ExtensionManaged` and `Manual` and schema-1 cache remain compatibility state. They do not grant a browser product authority. Production Native Messaging rejects browser settings mutation and native capture/control commands. A selected browser receives read-only Desktop snapshots and no extension-owned tray actions.

Protocol v2 requires an explicit connection intent, compatible capabilities, stable per-install Desktop identity, authenticated origin, and a registered browser/profile identity. Native consent and the browser's explicit installation pin are separate trust decisions. Registration never selects a default. Desktop selects a known target. A fresh lease then authorizes only media assigned to that target. Reconnect changes ephemeral connection and lease identities while preserving the explicit target selection. A retired connection cannot reclaim a newer same-profile connection's lease.

Every capture or recording persists its authority and delivery route before media side effects. `DesktopDelivery` freezes its local asset job, initial browser target, and post-capture choice preference. `assets/**` publishes only user-selected PNG/MP4 files through durable atomic jobs. A separate transfer-owned per-media browser assignment can bind the frozen job to one known target after capture. It cannot silently retarget an existing assignment.

A validated local folder is required before capture or recording.

Local publication remains the fallback when the browser is absent or no verified durable receipt arrives. The optional native destination prompt can select Local, a registered browser, or Later. Cancellation, failed publication, failed transfer, and rejected transfer preserve recoverable protected source media. Cleanup requires verified local publication for the same frozen job or an authorized browser receipt matching the source hash and total byte count. Timeouts do not prove durable publication. The [data-handling policy](../../security/data-handling.md) owns retention and disclosure rules.

`ModeEpoch` still fences immutable operation snapshots. The DTOs retain `ExtensionTransfer` and `LocalAsset` for legacy compatibility. New production capture uses the Desktop-owned route. `ShellActionId` remains native-only, separate from the explicit `ExtensionIntentActionId` mapping.

Recording pointer/click telemetry requires opt-in. It uses active time and captured-frame coordinates. Native code sanitizes and bounds telemetry. It validates retained data again before browser delivery. The extension converts milliseconds to seconds once. This does not authorize a local telemetry sidecar format.

## Trade-off

The local fallback permits capture with a closed browser and preserves user output across transfer failure. Protected recovery state can outlive a browser session and needs explicit recovery. Queue, media-size, disk-headroom, and parsing limits still apply. A selected trusted browser can read only its assigned media and can claim durable storage. The native process can verify the receipt fields but cannot independently prove the browser's physical storage durability. A compromised Windows user session remains outside the DPAPI and pipe boundary.

This decision does not authorize native accounts, gallery, editor, cloud sync, OCR, transcript, AI behavior, or a general file store. The independent design-system and native-sandbox UI goal remains active.

## Proof

Proof covers settings registry persistence and failed writes, Desktop authority, selection, and retired connections. It covers asset publication, crash recovery, durable-ACK mismatches, transfer replay and retention, telemetry frames, privacy, and redaction. Coverage floors, Windows smoke, and real-extension Windows E2E supplement these tests. Runtime acceptance requires collected Windows evidence. Deterministic tests alone do not establish it.
