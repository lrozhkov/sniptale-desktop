# Desktop design system

This index owns routing for Sniptale Desktop design. [Foundations](foundations.md) owns the proposed visual and interaction contract, [research](research.md) owns the source assessment, and the [delivery plan](delivery-plan.md) owns the implementation sequence and acceptance criteria.

## Product surfaces

The scope includes the compact capture panel, settings window, tray menu, recording controls, and region-selection presentation. The user selected a capture panel with screenshot/video and target selection. Existing settings, mode, recording, consent, and installation owners retain their authority.

Shell presentation belongs to `tray/**`. Region selection belongs to `capture/**`. [Module ownership](../architecture/module-ownership.md) owns those boundaries. Design work does not introduce account, gallery, editor, cloud, OCR, transcript, or AI surfaces.

Settings must distinguish Desktop drafts, saved settings, registered and selected browser destinations, unavailable settings, pending operations, and recoverable failure under [ADR 0005](../architecture/adr/0005-dual-settings-authority-and-local-asset-delivery.md). Appearance preferences must remain separate from capture authority. Tray presentation must distinguish idle, recording, unavailable actions, destination selection, and warnings without relying on color alone.

## Reference adaptation

The Sniptale browser repository supplies the reference structure: a routing `DESIGN.md`, focused documents under `docs/design`, and an interactive catalog under `tooling/design-reference`. Its foundations, selection, buttons, fields, choices, navigation, layers, compositions, localization, and verification documents inform this work.

Adapt the reference's quiet surfaces, semantic tokens, regular text, neutral commands, orange selection accent, complete press cycle, stable targets, draft recovery, and specimen-first workflow. Shared web components follow the reference geometry, keyboard behavior, and operating-system preferences. Tauri owns window integration. Specialized browser editors and their infrastructure are outside this scope.

## Current status

The browser sandbox imports the application components, layered styles, bundled font, and Fluent English/Russian catalogs. Browser proof covers compact capture controls and appearance settings. The first installed Tauri screenshot workflow passed. The full settings catalog and remaining native workflows remain incomplete. [Delivery plan](delivery-plan.md) owns current evidence and remaining acceptance. [Localization](localization.md) owns message and layout requirements. Do not label technical review as user visual acceptance.
