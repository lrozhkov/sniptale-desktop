# Desktop design system

This index owns routing for Sniptale Desktop design. [Foundations](foundations.md) owns the proposed visual and interaction contract, [research](research.md) owns the source assessment, and the [delivery plan](delivery-plan.md) owns the implementation sequence and acceptance criteria.

## Product surfaces

The scope is the settings window and tray menu. The existing settings, mode, recording, consent, and installation owners retain their authority. Presentation belongs to `tray/**` as defined by [module ownership](../architecture/module-ownership.md). Design work does not introduce account, gallery, editor, cloud, OCR, transcript, or AI surfaces.

Settings must distinguish Desktop drafts, saved settings, registered and selected browser destinations, unavailable settings, pending operations, and recoverable failure under [ADR 0005](../architecture/adr/0005-dual-settings-authority-and-local-asset-delivery.md). Appearance preferences must remain separate from capture authority. Tray presentation must distinguish idle, recording, unavailable actions, destination selection, and warnings without relying on color alone.

## Reference adaptation

The Sniptale browser repository supplies the reference structure: a routing `DESIGN.md`, focused documents under `docs/design`, and an interactive catalog under `tooling/design-reference`. Its foundations, selection, buttons, fields, choices, navigation, layers, compositions, localization, and verification documents inform this work.

Adapt the reference's quiet surfaces, semantic tokens, regular text, neutral commands, orange selection accent, complete press cycle, stable targets, draft recovery, and specimen-first workflow. Desktop implementation follows native Windows geometry, keyboard behavior, UI Automation, and operating-system preferences. Specialized browser editors and their infrastructure are outside this scope.

## Current status

The repository contains the concept and researched delivery plan. Component inventory, executable sandbox, shared native primitives, product rewrite, automated sandbox admission, and Windows acceptance evidence are pending. No design version has received user visual acceptance. Record that distinction when reporting progress.
