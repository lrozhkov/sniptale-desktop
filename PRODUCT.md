# Sniptale Desktop

<!-- impeccable:product-schema 1 -->

## Platform

web

## Stack

Tauri hosts one React and TypeScript interface. A browser runs the same components in the sandbox. The existing Rust owners retain capture, recording, publication, recovery, settings, and authorization. The user selected Tauri and authorized implementation without further design approval rounds.

## Purpose and operating context

Users capture screenshots and video through a compact floating toolbar. They choose a capture type and source, start capture, and receive progress, completion, or a recoverable error. The resident agent, tray, and hotkeys remain available when the panel is closed.

## Capabilities and constraints

Windows is the first capture integration. macOS and Linux require owner-local system adapters and packaging. The interface must distinguish unsupported features, missing permission, and unavailable sources. A fixture does not establish operating-system support.

The existing owner map and security contracts remain authoritative. Web components do not own recording state or publish files. An internal UI contract must not change the extension protocol.

## Brand commitments

Use the Sniptale design system from the Sniptale web repository. Use its bundled interface fonts, typography roles, icons, colors, and compact toolbar conventions. The user rejected a large settings window as the primary capture surface. Theme and language controls belong in a separate application settings window.

## Accessibility and localization

Use shared Fluent catalogs. Support keyboard operation, visible focus, expanded text, plural forms, right-to-left layout, and scaling. Keep native menu and file-dialog behavior explicit.

## Delivery

First complete the shared browser sandbox and one capture workflow. Then replace the remaining application surfaces. Validate browser behavior separately from operating-system integration. Complete normal review and closeout, push the result to GitHub, and produce alpha 0.1.1.
