# Desktop UI delivery plan

[Design index](index.md) routes the concept and research. This document records the next requested goal and its acceptance sequence. Execution is in progress. The separately requested Desktop/extension authority and recovery integration is an intermediate engineering stage. Its canonical contract is [ADR 0005](../architecture/adr/0005-dual-settings-authority-and-local-asset-delivery.md). The code-size policy update does not replace the UI objective. This plan does not mark the UI goal complete.

## Objective and scope

Deliver a repository-owned Sniptale Desktop design system. Demonstrate the required components and states in a working sandbox.

Then rebuild settings and tray presentation from the shared primitives. Enforce continued sandbox coverage for future UI changes. Preserve existing settings authority, security decisions, and reachable workflows. Use the explicitly requested integration contract in ADR 0005 as the behavioral baseline. Visual work must not introduce further authority changes.

## Delivery sequence

1. Complete the surface inventory and baseline. Map every existing settings field, theme/mode control, notice, command, dialog, tray item, and submenu to its owner, consumers, state transitions, and current Windows screenshot. Record current first-open, warm-open, input, scrolling, and resize timings with the benchmark method and guest resources. Inventory the actual locale support before deciding whether translation work is required.
2. Establish shared design inputs. Review the proposed foundations against the browser reference. Create one token source for colors, semantic type roles, spacing, geometry, focus, and motion. Generate or consume it consistently in the native sandbox and application. Verify output drift. Select the font through light/dark English/Russian specimens, accessibility, scaling, and license review. Keep choices explicit when the browser reference and native platform differ.
3. Build a vertical native sandbox spike before expanding infrastructure. Show a neutral button, draft checkbox, dropdown, editable value, navigation item, hotkey field, feedback region, and native menu using the actual Windows implementations. Prove keyboard routing, focus, UI Automation names/patterns, DPI/text scaling, theme changes, and reduced motion. If this approach cannot meet acceptance, document the measured limitation and compare alternatives before committing to a runtime change.
4. Expand the catalog to every supported component and required state listed below. Provide an indexed local launch command through the repository wrappers, deterministic fixture selection, a visible specimen version, and a link to the owning design rule. Use fake typed data and in-memory drafts. Sandbox actions must not capture media, register global hotkeys, or change installed settings. They must not start browser integration or request installation repair.
5. Demonstrate component behavior and full settings/tray compositions before rewriting the application surface. Supply runnable previews, settled screenshots, interaction evidence, and open visual decisions. Record exactly which version was demonstrated and any user feedback. Automated tests or expert review must not be labelled user visual acceptance. Incorporate feedback into shared rules and specimens before product composition.
6. Rebuild the settings presentation from demonstrated primitives. Preserve Desktop settings authority and legacy profile compatibility, Apply/Cancel/discard semantics, custom persisted values, appearance persistence, device refresh, hotkey conflicts, and error recovery. Migrate one coherent section at a time, retaining the existing settings owners and typed commands. Update the tray presentation with the same semantic hierarchy while preserving native menu behavior and command dispatch.
7. Enforce the workflow. Add a machine-readable component/case inventory and negative fixtures. Require changed UI components to reference covered cases and their canonical rule. Run deterministic inventory, token drift, and presentation tests in the existing fast QA flow before compilation and VM admission. Validate actual native interaction in the Windows lane. Cover missing specimens and stale references without pretending that a file-name check proves UX quality. Route future UI tasks through `DESIGN.md` and the test policy.
8. Close with deterministic owner tests, applicable coverage, architecture/security review for changed boundaries, and real Windows interaction evidence. Run required wrappers and closeout. Record performance results, supported configurations, unresolved limitations, and the exact reviewed specimen/product versions. Prepare the resulting reviewable PR with no claim of runtime acceptance until the Windows checks pass.

## Required specimen inventory

| Family | Required cases |
| --- | --- |
| Foundations | Light, dark, system theme, system contrast colors, reduced motion, typography, semantic colors, focus, disabled state, long text. |
| Commands | Neutral primary/secondary, icon action with accessible name, hover, focus, press/hold, drag outside, release outside, interruption, duplicate activation prevention, unavailable reason. |
| Draft values | Checked/unchecked/disabled checkbox, radio/exclusive selection where used, dropdown closed/open/keyboard selection, numeric or custom persisted value, destination display and folder-picker handoff. |
| Hotkeys | Idle, focused, capturing, accepted, invalid, conflicting, cancelled, unavailable, focus restored after capture. |
| Navigation | Selected versus hovered, keyboard movement, resize, scrolling, preserved draft, short work area, focus kept visible. |
| Feedback and layers | Initial/unavailable state, pending, success, recoverable failure/retry, dirty-close confirmation, Escape, continued editing, focus return, long error text. |
| Compositions | Capture, Video, Audio, Shortcuts, appearance controls, Local-folder entry/active draft, registered/selected browser destinations, unavailable settings, authority changed while open. |
| Tray | Idle/recording, checked mode, unavailable action, warning, shortcut label, submenu, keyboard activation/dismissal, work-area edge placement, reopening without stale state. |

Every component actually used by the product needs an inventory entry. Optional future components do not justify building unrelated controls. Each entry identifies its rule, real implementation, deterministic cases, applicable Windows cases, and required evidence. State coverage must include both local control behavior and the owner contract it projects.

## Acceptance and measurements

- A documented wrapper starts the catalog locally. Operation requires no installer, active browser connection, real capture, or changes to user settings. The Windows specimen uses the production primitives and fixture-driven presentation. Rules and tokens have one source of truth.
- Both themes and contrast themes remain usable with keyboard, Narrator, and UI Automation. Test 100%, 150%, and 200% display scale, plus independent 200% text scale, long English/Russian specimens, and the smallest supported work area. Validate actual application-supported languages separately from localization stress specimens.
- Apply failures preserve drafts. Invalid input remains editable. Cancellation, dirty-close decisions, focus return, scrolling, and unavailable states work in the real settings window. Tray actions remain reachable and dispatch once. No native authority or consent boundary changes as a side effect of visual work.
- Record cold and warm sandbox startup and check time, cold/warm settings opening, event-to-visible-feedback latency, scroll/resize frame time, idle CPU, and additional private memory. Report sample count, median and p95 where applicable, display refresh rate, guest resources, source revision, build profile, and fixture. Initial targets require p95 input feedback below 100 ms. They require p95 active animation work within 16.7 ms per frame on a 60 Hz reference guest. Investigate regressions against the measured baseline before claiming smoothness.
- Routine token, fixture, inventory, and presentation checks require neither app compilation nor VM startup. Reuse the existing warmed guest/session for actual native verification and explain when a clean guest is needed. Measure the incremental edit/check/preview loop and publish the result rather than assuming the sandbox improves it.
- Negative tests reject missing component cases, broken inventory references, token drift, and incorrect owner-state projection. Behavioral fixes receive meaningful regression tests. Formatting and palette data edits do not receive tests that merely repeat their implementation.
- New dependencies, fonts, or tools follow the existing advisory, license, and seven-full-day release-age policy. No new runtime or QA dependency is selected merely for visual novelty.

## Completion record

The Windows integration baseline exposes owner-drawn settings buttons without UI Automation InvokePattern or successful UIA SetFocus. Its diagnostic driver uses pointer activation of visible controls and keyboard selection in native lists, then verifies owner persistence. That is not UI Automation accessibility acceptance. The native sandbox spike must resolve and prove the supported names, roles, patterns, and focus before product migration.

Research and initial concept: prepared. Native presentation source inspection has started.

A Windows development session is being prepared. Existing white-on-orange contrast is 3.56:1 in light theme and 2.80:1 in dark theme, below the normal-text target. This is a baseline finding, not an accepted design.

Complete inventory, shared token implementation, native spike, full sandbox, product migration, enforcement, measured Windows proof, and final closeout: pending. Update this record as evidence is obtained. Keep logs and screenshots as evidence rather than a source of design rules.
