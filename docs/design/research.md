# Desktop UI research

Assessment date: 2026-10-31. [Design index](index.md) routes the resulting concept and implementation plan. Sources below were checked for this task. Their examples must be interpreted for the actual Desktop implementation rather than copied as framework requirements.

## Actual stack

Desktop uses Rust with the `windows` crate. The settings adapter uses Win32 HWND controls, GDI control drawing, Direct2D surface drawing, and DirectWrite text. It already has platform-neutral presentation and draft models, a light/dark/system-contrast palette, and DPI-independent layout helpers under `src/tray/settings_window`. Tray commands use typed menu models and a native Windows menu adapter under `src/tray/menu`.

The relevant implementation question is how to make this native surface consistent, accessible, smooth, and easy to preview. The current evidence does not establish a need to replace the runtime with a web shell or a different UI framework. Reconsider the rendering choice only if the native component spike cannot meet the measured interaction and accessibility criteria.

## Sources and decisions

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

Adopt that workflow and those semantic distinctions. Copying the HTML specimens into Desktop would not prove native focus, UI Automation, per-monitor DPI, or Windows menu behavior. The native sandbox must exercise the same components and presentation model as the application. A lightweight visual catalog can complement it for browsing rules and comparing concepts, without becoming an independent production component implementation.

## Observations requiring a spike

Existing Desktop colors already belong to the Sniptale family, but the palette tests assert literal values rather than contrast across states. Existing layout code contains fixed row and page heights, and GDI fonts are sized in points while DirectWrite text uses DIPs. These observations justify testing scaled typography, clipping, composition, and shared tokens. They do not by themselves establish that a particular screen fails.

The browser reference combines token CSS with newer group-specific design decisions. Resolve geometry and semantic differences explicitly in Desktop foundations rather than mechanically copying every CSS constant. Circular checkbox drawing and glass treatments in the browser material include proposed decisions. They are not automatic Desktop acceptance requirements.
