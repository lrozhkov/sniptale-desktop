# ADR 0008: Shared web presentation through Tauri

Status: accepted on 2026-11-02. The first complete Windows workflow is verified. The [delivery plan](../../design/delivery-plan.md) records proof and remaining release checks.

## Decision

Use one web interface and the existing Rust core. Tauri owns the application window and the internal bridge. Existing owners retain product rules and operating-system adapters. This decision extends the [owner map](../module-ownership.md). It does not replace it.

The frontend lives in `ui/`. Application and browser sandbox import the same components, tokens, fonts, and Fluent catalogs. A small typed client separates presentation from transport. Only the application client imports Tauri. The sandbox client provides explicit, controlled fixtures.

Rust owns recording state, command admissibility, settings, publication, and recovery. The frontend retains display preferences and unsaved input. It does not implement another recording state machine. Tauri commands validate input and call coordination in `app/**`. They do not contain capture or storage policy. The public extension protocol remains independent.

Keep system adapters in their existing owners: capture in `capture/**`, recording in `recording/**`, audio in `audio/**`, and global shortcuts in `hotkeys/**`. Platform-neutral models contain no Windows handles, COM objects, registry paths, or platform-specific source identifiers. Extract interfaces only for real operations and lifecycles.

The backend reports feature support, system permission, and source availability separately. Components use those facts instead of testing operating-system names. macOS and Linux remain unsupported until their actual adapters and integration proof exist.

## Lifecycle and security

Keep one resident runtime and one authority for tray, shortcuts, and recording. Closing a web panel hides presentation. It does not stop the agent or an active recording. Reopening requests the current Rust snapshot. Do not launch a second resident runtime from Tauri.

Bundle trusted local frontend assets. Restrict Tauri capabilities to required windows and operations. Do not grant remote origins access to application commands. Validate commands in Rust even when the corresponding control is disabled. Media streams do not pass through UI JSON. Existing storage, IPC authorization, and extension-origin checks remain in force.

[Official Tauri capabilities](https://v2.tauri.app/security/capabilities/) define bridge permissions. [Official API mocking](https://v2.tauri.app/develop/tests/mocking/) supports browser-side transport tests. Neither mechanism replaces owner validation or proves capture support.

## Migration and Win32 retirement

First complete one vertical scenario: floating capture toolbar, request dispatch, authoritative progress, and completion or recoverable failure. Demonstrate the shared components with browser fixtures. Then connect the same components to Windows capture through the existing coordination.

Keep the installed Win32 UI as a temporary fallback until the corresponding Tauri scenario passes its integration checks. Remove each replaced presentation adapter after that proof. Preserve reusable Rust models, owner tests, settings authority, capture adapters, and recording adapters. Do not continue developing the abandoned native component catalog or its FlaUI interaction driver. Retire migration-only dependencies and tooling when the web sandbox replaces them.

A browser fixture is marked as a simulation and does not publish files. Test component behavior in the browser. Use targeted Windows checks for window focus, dragging, tray lifecycle, permissions, capture, and installed provenance. A style change must not require Rust, EXE, or MSI builds.

## Acceptance

The browser sandbox starts with one documented command. The application imports its demonstrated components. Windows capture completes through the existing owners. Closing and reopening the panel preserves backend state. Unsupported capabilities have clear explanations. Existing security and extension contracts remain valid. These are required proofs, not properties guaranteed by selecting Tauri.

## WebView and resource acceptance

Use the system WebView. Do not bundle a complete WebView runtime without an explicit offline-installation requirement. On Windows, document the shared WebView2 prerequisite and any bootstrap download separately from the application package.

Measure installer bytes, installed application bytes, additional runtime bytes, and idle RAM as separate values. Record the OS, runtime version, measurement method, and cold or warm conditions. Do not report only the Rust executable size as the application footprint.

Measure the resident agent before opening the panel, with the panel open, and after closing it. Verify release of the UI renderer and its resources while the Rust agent and any active recording continue. Hiding a live WebView is not evidence of resource release. The intended close behavior destroys the presentation window and WebView, preserves backend state, and recreates presentation from the current snapshot when reopened.
