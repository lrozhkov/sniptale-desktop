# Desktop foundations

[ADR 0008](../architecture/adr/0008-tauri-presentation-boundary.md) owns the shared Tauri/web boundary. The [delivery plan](delivery-plan.md) records implementation and proof. Use the Sniptale web design system as the visual reference.

## Capture surface

Use a compact floating toolbar as the primary capture surface. Make capture type, source, primary action, and recording state available without a settings window. Put appearance and language in a separate application settings window. Combine capture source and per-recording quality in one compact popup.

Keep ordinary commands neutral. Orange identifies an active icon tool with a second structural cue. Labeled exclusive choices use neutral selected surfaces. Keep hover, press, selection, expansion, focus, and pending state separate. Show keyboard focus indicators only during keyboard interaction. A mouse click must not show a keyboard frame. Use one primary command per task region.

## Shared visual inputs

`ui/src/styles/tokens.css` owns web colors, type defaults, radii, and motion values. It follows the Sniptale web semantic palette. Use the warm light canvas and near-black dark canvas. Consumers use semantic roles instead of local palette copies.

Use the bundled Golos UI face through the stable Sniptale Interface family. Keep the exact upstream font license. Use regular body text and restrained heading emphasis. Interface family, size, line height, weight, and foreground remain independent roles. Measure actual strings when selecting control geometry.

Declare shared CSS layer order before component styles. Shared tokens also own compact control geometry and theme-preview colors. Press feedback moves content without moving selected surfaces or hit targets. Dismiss controls keep neutral chrome. Unselected quality choices show hover feedback. Use one consistent icon family. Preserve icon geometry during hover and expansion. Use the shared chevron component and reserved selection-check column. Preserve compact toolbar geometry and the source system's quiet utility controls. Do not create decorative dashboards, oversized selectors, or repeated branding inside forms. The sandbox chrome is documentation. It is not the application window.

## Accessibility and adaptation

Normal text requires 4.5:1 contrast. Large text requires 3:1. Essential control marks and focus indicators require 3:1 against adjacent colors. Evaluate actual composited surfaces. Branding yields to accessibility and forced colors.

Support English, Russian, expanded text, RTL, and narrow viewports. Scale the whole toolbar, including icons and spacing. Use logical inline/block geometry. Keep essential actions reachable without horizontal page scrolling. Provide full accessible values when visible text is shortened. [Localization](localization.md) owns messages and language fallback.

Use semantic HTML controls with complete accessible names. A toolbar has one Tab entry and arrow/Home/End navigation. Moving focus does not activate a tool. Menus support keyboard opening, item navigation, Escape, outside dismissal, and focus return. A native system dialog keeps its platform lifecycle.

## Actions and state

Activate pointer commands once on release inside the target. Cancellation, drag outside, interruption, and release outside must not dispatch. Preserve corresponding Space, Enter, Escape, Tab, and Shift+Tab behavior.

Rust supplies operation state and command availability. Pending UI prevents duplicate interaction without replacing backend admission. A failed action preserves relevant input and explains recovery. Show status outside the toolbar layout. A status change must not move the toolbar. Do not expose raw backend errors or payloads.

Use checkboxes for draft values that require Apply. Use switches only for immediate settings. Preserve existing Apply, Cancel, persistence, and destination-selection timing. A failed Apply preserves the draft. Closing a dirty draft offers a clear discard decision and restores focus when editing continues.

## Motion and system integration

Use short local transitions for state changes. Do not delay input, move hit targets, or add decorative idle animation. Respect reduced motion, contrast, transparency, and system theme preferences. Keep focus and valid input across theme, size, and language changes.

System menus, tray behavior, file dialogs, and permission prompts remain explicit integration boundaries. Browser screenshots do not prove their behavior. Closing the capture panel releases presentation resources while the Rust agent continues. Reopening projects the current backend snapshot.
