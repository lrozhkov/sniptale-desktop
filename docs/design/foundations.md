# Desktop foundations

[Design index](index.md) routes related documents. These rules define the target design; the [delivery plan](delivery-plan.md) records implementation status and proof.

## Character and hierarchy

Use a compact, calm settings interface with clear section names, readable current values, and visible recovery actions. Prefer one clear primary task per region. Parameter names and explanatory metadata are secondary to values and meaningful state. Use sentence case. Avoid decorative dashboard content, large empty hero sections, and repeated branding inside forms.

Keep ordinary commands neutral. Orange identifies the accepted selection or active mode where a second structural cue also exists. Error and warning tones require semantic text and their own contrast proof. Hover, keyboard focus, selection, expansion, physical press, and pending work are distinct states.

## Semantic colors

Preserve the Sniptale family: warm off-white canvas and near-white panels in light theme; near-black canvas and charcoal panels in dark theme. Current family anchors are light canvas `#f6f2ed`, dark canvas `#09090b`, light accent `#ea580c`, and dark accent `#f97316`. These anchors are brand inputs, not proof that every foreground pairing is readable.

Define separate semantic roles for canvas, panel, input, hover, pressed, boundary, primary text, secondary text, selected text, focus, accent, warning, and error. A shared token owner must supply product and sandbox. Consumers must not introduce feature-local palette copies. Composite translucent layers before evaluating contrast or sending colors to opaque GDI controls.

Normal text must reach 4.5:1 contrast, large text 3:1, and essential control boundaries, state marks, and focus indicators 3:1 against adjacent colors. Evaluate the actual foreground and composited background. Orange fills with white text are not assumed to pass. Validate all interactive states. Contrast themes use system colors and visible state marks; branding must yield to accessibility.

## Typography and geometry

Use semantic type roles and a shared spacing scale. Initial specimens compare Segoe UI Variable, already used by Desktop, with the browser reference's Golos UI before the final font decision. Do not redistribute a font without its authoritative license and provenance. Prefer regular control text and restrained heading emphasis. Never animate font weight if it moves surrounding content or changes target geometry.

Prototype a 4-DIP spacing scale, ordinary control bands of at least 32 DIP, 16-DIP icons, 4-DIP control radii, and 8-DIP dialog radii. These are starting values for visual comparison, not permission to replace native menu metrics or clip scaled text. Treat GDI point sizes and DirectWrite DIP sizes as different units; convert through the shared type roles. Measure actual text rather than deriving row height from a fixed English label.

Layouts must respond to client size, per-monitor DPI, and Windows text scale independently. Keep essential actions reachable at the smallest supported work area. Allow vertical scrolling and multiline labels when necessary; avoid horizontal scrolling for the settings form. Long paths may elide visually only when the full value remains accessible. Provide long English and Russian specimens even before deciding whether localization is part of the product rewrite.

## Commands, values, and navigation

Activate a pointer command once on release inside its target. Press, hold, drag outside, cancel, interruption, and release outside must not trigger an accidental command. Preserve native Space, Enter, Escape, arrow, Tab, and Shift+Tab behavior for the corresponding control role. Focus remains visible for keyboard use and must follow a coherent route through enabled controls.

Use checkboxes for values in a draft that requires Apply. Use switches only for a setting whose change commits immediately. A disabled checked value stays checked. Explain unavailable controls when the reason is necessary to proceed. Keep selected navigation distinct from hover. Dropdowns expose accepted values and support native keyboard selection; hotkey entry exposes capture, cancellation, invalid combination, and conflict states.

A presentation change must preserve the existing timing of Apply, Cancel, theme persistence, and mode changes. Invalid input remains visible for correction. A failed Apply preserves the draft and exposes a useful retry path. Pending work prevents duplicate effects. Closing a dirty draft offers a clear discard decision and restores focus when editing continues. Do not silently turn a draft-backed value into an immediate write.

## Motion and responsiveness

Respond to input immediately; animation must not delay activation or block keyboard interaction. Start with short color and opacity transitions in the 100–150 ms range and section transitions no longer than 200 ms. These are project targets to measure in the sandbox, not timings mandated by Microsoft. Avoid bounce, layout animation, moving hit targets, idle animation loops, and full-window redraws for a local hover change.

Respect Windows animation and transparency preferences and contrast themes. Reduced motion removes optional interpolation and displacement while preserving clear state feedback. Recover rendering resources after device loss. Window resize, theme changes, and monitor moves must retain draft, focus, and scroll context where valid.

## Native semantics

Retain standard Windows control roles and input semantics where they meet the interaction requirement. Owner drawing must preserve accessible names, role, value, enabled state, selection, keyboard focus, and relevant UI Automation patterns. Custom-painted interactive regions require an explicit accessibility implementation and Windows proof. A visually correct screenshot does not establish that proof.

The tray remains a native menu with system placement, dismissal, keyboard routing, checked and radio items, and submenu behavior. Share command ordering and typed state with the sandbox. Native menu colors and motion follow the operating system unless a demonstrated product requirement justifies a separately reviewed custom implementation.
