# ADR 0005: Dual settings authority and local asset delivery

Status: Accepted, 2026-07-11.

## Decision

`OperatingMode` is either `ExtensionManaged` or `Manual`; exactly one settings authority is active. ExtensionManaged accepts versioned extension snapshots and uses `ExtensionTransfer`. Manual uses the versioned local profile and `LocalAsset`, admits no controller pipe session, and sends no media or metadata through Native Messaging.

Every capture or recording persists its `ModeEpoch`, authority, and delivery route before media side effects. The snapshot remains immutable through the terminal state. Mode switches fence ingress, wait for bounded owner acknowledgements, and fail while an operation is active or finalizing.

`assets/**` publishes only user-selected PNG/MP4 files through durable atomic jobs. `ShellActionId` is native-only; extension intents use `ExtensionIntentActionId` and an explicit mapping. Manual v1 disables telemetry because no approved local sidecar format exists.

## Trade-off

Manual mode remains usable without extension availability, but it creates plaintext output at the selected destination and adds a second settings authority. Mutual exclusion, frozen routes, destination validation, and owner separation prevent silent authority mixing. The decision does not authorize accounts, gallery, editor, cloud sync, or a general file store.

## Proof

Settings mode/profile tests, asset publication/recovery tests, app ingress tests, redaction tests, coverage floors, and Windows desktop E2E scenarios cover transitions and route isolation.
