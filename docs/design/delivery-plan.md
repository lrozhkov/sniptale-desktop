# Desktop UI delivery plan

[ADR 0008](../architecture/adr/0008-tauri-presentation-boundary.md) owns the Tauri migration boundary. The user selected this approach on 2026-11-02. The native Win32 component-catalog strategy is superseded. The UI goal remains active.

## Required result

Deliver one shared web interface for Tauri and the browser sandbox. Use the Sniptale design system and bundled fonts from Sniptale web. The primary capture surface is a compact floating toolbar. Application settings use a separate window. The capture popup contains source and per-recording quality.

Keep the existing Rust owners, security contracts, extension protocol, publication rules, and recovery. Prepare the presentation for Windows, macOS, and Linux. Mark unsupported platforms and sources accurately. A fixture does not establish operating-system support.

## Implementation order

1. Record migration boundaries and Win32 retirement before implementation. ADR 0008 records this decision.
2. Implement the shared browser sandbox and typed client. Reuse application components, tokens, fonts, and Fluent catalogs. Verify compact layout, real interaction, keyboard use, light/dark themes, English/Russian, expanded text, RTL, and scaling.
3. Complete one real Windows workflow: toolbar, capture request, authoritative progress, and completion or recoverable failure. Commands remain thin adapters to `app/**`. Existing owners perform capture and publication.
4. Verify close and reopen behavior. The agent and recording survive panel closure. Reopening receives the current Rust state. Measure release of WebView resources.
5. Transfer remaining settings, recording controls, notices, and consent presentation through the same boundary. Preserve existing models and owner tests. Remove each replaced Win32 adapter after its corresponding integration proof.
6. Run browser checks through the shared local and CI checkpoint. Keep operating-system proof separate and targeted. Styling changes must not rebuild Rust or installers.
7. Complete required review, release checks, and normal closeout. Push the changes to GitHub and produce alpha 0.1.1. Do not mark the goal complete before release evidence exists.

## Current evidence

The shared web toolbar uses the bundled Golos UI font and one layered style system. Browser proof covers English light/dark, Russian at 640 pixels, and a 390-pixel layout. Expanded RTL at 200% scales icons, text, and spacing together. These layouts have no horizontal page overflow.

Controlled fixtures cover screenshot success, pending capture, permission and save failures, unsupported capture, recording pause, hide/reopen, resume, discard cancellation, and stop. Checks also cover pointer and keyboard focus, cancelled presses, repeated Enter, popup toggling, stable toolbar position, and separate settings synchronization. These results establish browser behavior. The resident now connects Tauri commands to its existing bounded tray queue. The first installed Windows capture workflow passed through real Tauri commands.

Independent visual review accepted the browser corrections. The extra drag-handle Tab entry was corrected and accepted. The sandbox includes shared General, Capture, Video, Audio, and Shortcuts settings components. Browser fixtures demonstrate draft editing, Apply, Cancel, save failure, pending operations, and unavailable settings. The application connects these forms to Rust through the bounded tray queue. Profile validation and the existing mode barrier control Apply. Windows proof covers profile Apply, persistence, native window close, and reopening. Native folder selection and the remaining settings integration still require release proof.

The common checkpoint now includes the frontend build, contract tests, and existing browser matrix. CI prepares the locked UI dependencies and matching Chromium revision. These checks do not start the VM.

The Windows-target Tauri adapter compiles. It defines typed commands, local-window authorization, scoped events, window destruction, and a returning event loop. The Windows resident now starts the Tauri event loop on its main thread. The Rust command entrypoint delegates to existing Manual coordination. The replaced Win32 capture/settings renderers and native catalog are removed. The folder picker, tray, consent, and settings validation remain with their existing owners. Native Tauri captions and frame theme now follow the application preferences. Windows-target Clippy passed with warnings denied. Exact capability variants remain available for future platform permission adapters. Their expectations include reasons and fail when stale.

Capture, recording, and asset workers now supply coalesced presentation facts. A local save requires the matching publication receipt. Transfer finalization does not claim durable publication. Parallel screenshots preserve active recording controls.

Commands carry an operation epoch. The Rust entrypoint rejects stale controls before worker dispatch. The recording worker resolves its active recording identity. Per-recording quality changes the frozen request without changing the stored profile. The shared toolbar reads its initial mode and quality from backend state.

Rust owner tests, DTO rejection tests, browser behavior, and Windows-target test compilation cover these changes. The installed Windows screenshot workflow passed. A replaced operation closes an obsolete discard confirmation. Queue tests cover concurrent requests, queue saturation, and a lost owner reply. The settings owner now persists theme and locale together. Partial writes preserve the other preference. The application publishes confirmed preference changes to both windows.

The shared destination form selects local output or a registered browser. Its Apply and Cancel affect delivery preferences independently. Profile Apply preserves unsaved delivery input. The existing registry validates stable target IDs and persists explicit choices. Recording blocks delivery changes. [ADR 0005](../architecture/adr/0005-dual-settings-authority-and-local-asset-delivery.md) keeps compatibility modes outside the new UI and preserves Desktop authority.

The recording worker drives the existing owner progress cadence during active recording. Idle workers block without polling. Tests cover progress without commands, a busy queue, terminal identity release, and publication failure. Existing service tests cover pause, resume, and duration-cap finalization. Source selection and asset preparation do not consume active recording time. Windows proof covers a running clock, pause, resume, stop, durable MP4 publication, discard cancellation, confirmed discard, and panel close/reopen during recording. Native duration-cap and complete media regression remain part of release acceptance.

The toolbar now opens the separate application settings window. Rust reports missing folder setup and removes Start until configuration exists. Safety controls remain available for active recording. The shared UI distinguishes owner refusal from connection failure and provides explicit connection retry. Unsupported capability takes precedence over missing setup.

A submitted command hides the previous operation outcome and locks source and mode controls. The client reads the authoritative snapshot after command admission. It does not repeat the media command if that read fails. A controlled browser regression held a new command after an earlier save. The test failed before this correction and passed after it. The full browser matrix, independent visual review, and installed Windows workflow accepted this presentation behavior.

The capture window now uses a transparent background and fits the shared toolbar. The Rust geometry adapter reserves space for popups, feedback, and confirmation dialogs. It uses the monitor work area and WebView pixel ratio. Pure geometry tests verify edge placement, negative monitor coordinates, centering, and anchor preservation. Browser proof covers the real desktop host through a Tauri IPC mock. Native positioning and click-through remain unverified.

Active-window capture uses the existing Windows foreground owner. Shared state contains only source availability. Rust refuses a missing window before capture side effects and restores the panel. Owner tests verify source mapping and refusal. Browser proof verifies the explanation, disabled Start, and recovery through another source. Independent visual review accepted this browser scope. Installed Windows proof published one PNG of a synthetic external window and verified its physical bounds and center color.

Browser proof also exercises the application settings adapter through the official Tauri IPC mock. It covers owner refusal, retry, Apply, ordered preference writes, and unsaved-close confirmation. These mocks do not establish operating-system behavior.

Windows proof uses the existing pinned Playwright client through WebView2 CDP. The isolated guest process receives the debugging environment. Normal application startup does not enable CDP. UI Automation discovers the native frame. Playwright operates the real components and Tauri client.

The confirmed workflow selects a screen, publishes exactly one PNG, displays the Rust save receipt, destroys the panel, and restores saved state after reopening. The agent remains alive. The same run passed network isolation, resident, tray, and region capture. Collected development evidence has verified fingerprints. It is not public-release acceptance.

The ordinary native Windows build passed 1049 owner tests. One existing live-consent test was ignored. The complete development workflow passed on 2026-11-03 in session `20261102T235250Z`, iteration `20261103T011949Z`. Collection returned `verified: true`. Network isolation, resident startup, tray, screen capture, region capture, active-window capture, settings, recording controls, and WebView release passed. Subsequent UI corrections used diagnostic preview builds and retained the ordinary Rust test proof. This development receipt has `release_admissible: false`. It does not replace clean release acceptance. Native edge positioning and click-through remain unverified. Queued recording controls retain the observed recording identity. Owner tests reject a stale discard after a replacement recording starts.

Guest synchronization preserves exact Tauri-generated permission and schema directories. Local negative tests cover deletion, prefix lookalikes, and timestamp stability. Subsequent script-only builds reuse the Rust artifact. Caller-scope and driver rejection tests run locally before another OS scenario.

The compile-only Windows cross-check omits generated presentation context and cannot start the UI. Native Windows builds must generate the complete context, ACLs, and resources. The system WebView remains external to the application bundle. Guest source bundles include built web assets and their font. Missing web output fails before guest compilation.

## Resource acceptance

Use the system WebView. A full bundled WebView requires an explicit offline-installation requirement. Measure installer size, installed application size, additional runtime size, and idle RAM separately. Record measurement conditions and runtime versions.

Measure the agent before opening UI, with the panel open, and after closing it. Verify UI resource release while the agent and any recording remain active. A hidden live WebView does not satisfy this proof.

The confirmed development workflow measured the following working sets. These are observations from the isolated Windows guest. They are not release limits. WebView working set is the sum of its child-process working sets and can count shared pages more than once. The active-recording agent includes capture and encoder resources.

| State | Agent working set | WebView processes | WebView working set |
| --- | --- | --- | --- |
| Idle before panel | 24.5 MiB | 0 | 0 |
| Panel open after settings close | 51.4 MiB | 6 | 343.0 MiB |
| Panel closed while recording continues | 169.3 MiB | 0 | 0 |

The workflow reopened the panel and completed the same recording after WebView release. Installer size, installed payload size, shared runtime footprint, and final release measurements remain required.

## Verification limits

Use browser tests for component behavior. Use targeted OS tests for focus, dragging, tray lifecycle, permissions, capture, and installed provenance. Run a full package regression at release acceptance. Repeat a diagnostic scenario only after a specific change or identified external failure.
