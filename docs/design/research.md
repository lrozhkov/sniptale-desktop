# Desktop UI research

The user selected Tauri on 2026-11-02. [ADR 0008](../architecture/adr/0008-tauri-presentation-boundary.md) supersedes the native-catalog strategy. The earlier Win32 assessment below is historical evidence. New component work belongs in the shared web interface.

Assessment date: 2026-10-31. [Design index](index.md) routes the resulting concept and implementation plan. Sources below were checked for this task. Their examples must be interpreted for the actual Desktop implementation rather than copied as framework requirements.

## Current stack

Desktop uses one React and TypeScript interface hosted by Tauri. Rust owners retain capture, recording, settings, storage, and recovery. The browser sandbox imports the application components. Windows keeps native tray, consent, source selection, and system dialogs. The replaced Win32 settings and capture renderers are retired.

The [presentation boundary](../architecture/adr/0008-tauri-presentation-boundary.md) owns this decision. The [delivery plan](delivery-plan.md) records actual proof and remaining release checks.

## Historical native assessment

| Primary source | Guidance that applies here | Desktop consequence |
| --- | --- | --- |
| [Microsoft Windows app best practices](https://learn.microsoft.com/en-us/windows/apps/get-started/best-practices) | Integrate keyboard access, UI Automation, contrast themes, text/display scaling, and accessibility into release criteria. | Test actual Windows behavior, including 200% text scale. Platform-neutral tests supplement this proof. |
| [Windows typography, updated 2026-04-14](https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/typography) | Use readable type hierarchy, regular text, sentence case, and Windows font support. | Segoe UI Variable is a strong native candidate. Compare a real specimen with the Sniptale reference before selecting the final face. |
| [Per-monitor DPI for desktop applications](https://learn.microsoft.com/en-us/windows/win32/hidpi/high-dpi-desktop-application-development-on-windows) | Per-monitor awareness requires explicit Win32 layout work and DPI-change handling. | Recompute geometry and resources when moving monitors. This is separate from text scaling. The page also contains historical UWP advice, which is not a reason to migrate this existing app. |
| [Microsoft custom UI accessibility](https://learn.microsoft.com/en-us/windows/win32/winauto/custom-user-interface-elements) | Standard controls expose baseline accessibility. Owner drawing has conditions, and fully custom controls require additional work. | Preserve native semantics and inspect UI Automation after styling. Do not infer accessibility from the control's visual shape or underlying class alone. |
| [Windows contrast themes](https://learn.microsoft.com/en-us/windows/apps/design/accessibility/high-contrast-themes) | Use system semantic colors and preserve recognizable control/state boundaries in contrast themes. | Branded light/dark themes need a separate system-color projection and runtime theme-change proof. |
| [Windows motion](https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/motion) and [Fluent 2 motion](https://fluent2.microsoft.design/motion) | Motion should explain changes, respond to input, and remain consistent with the platform. | Use brief local transitions with stable targets and immediate activation. Benchmark them rather than adding perpetual repainting. |
| [SystemParametersInfoW animation preference](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-systemparametersinfow) | `SPI_GETCLIENTAREAANIMATION` reports whether client animations are enabled. | The native motion policy must honor the operating-system setting and refresh when it changes. |
| [W3C text contrast](https://www.w3.org/WAI/WCAG22/Understanding/contrast-minimum.html) and [non-text contrast](https://www.w3.org/WAI/WCAG22/Understanding/non-text-contrast.html) | Text, necessary state marks, and control boundaries require appropriate contrast. | Adopt measurable visual criteria as a Desktop design policy. Do not claim complete WCAG compliance from these checks. |

## Browser reference assessment

The sibling Sniptale repository's `DESIGN.md` routes to `docs/design/index.md`. Its `tooling/design-reference` contains a local interactive catalog with groups, required cases, shared tokens, and direct links to the canonical rules. Its verification document requires real pointer and keyboard operation, full press cycles, error recovery, long text, themes, reduced motion, and composition checks.

Use the shared component workflow for Desktop. Browser checks cover pointer and keyboard input, delayed responses, failure recovery, themes, localization, scaling, and composition. Windows checks cover actual windows, source selection, tray, capture, recording, and installed provenance. Keep one implementation of each application component.

## Historical spike observations

The earlier native palette tests checked literal colors. They did not measure contrast across every state. GDI and DirectWrite used different units. These observations justified typography and composition checks. They did not establish a defect in a specific screen.

The browser reference combines token CSS with newer group-specific design decisions. Resolve geometry and semantic differences explicitly in Desktop foundations rather than mechanically copying every CSS constant. Circular checkbox drawing and glass treatments in the browser material include proposed decisions. They are not automatic Desktop acceptance requirements.

## Current test clients

Playwright drives application components in the browser. In the isolated Windows guest, it connects to the real WebView2 through a process-local CDP endpoint. Normal application startup does not enable CDP. The [delivery plan](delivery-plan.md) records the confirmed workflow and its limits.

System UI Automation remains for native frames and system dialogs. It does not drive web controls. The native component catalog, its command-line mode, and its FlaUI dependencies are retired. Existing Rust owner tests continue to verify product rules. Clean Windows acceptance verifies the release package.
