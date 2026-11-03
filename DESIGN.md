# Sniptale Desktop design

Use the [design index](docs/design/index.md) and the [Tauri presentation boundary](docs/architecture/adr/0008-tauri-presentation-boundary.md). The desktop interface follows the Sniptale web design system from Sniptale web.

The primary capture surface is a compact floating toolbar. Put theme and language controls in a separate application settings window. Keep capture type, source, primary action, and recording state immediately available. Settings are a secondary surface.

Application and browser sandbox import the same components, semantic tokens, bundled fonts, and Fluent messages. Only their typed backend clients differ. Do not create a separate HTML imitation for the sandbox.

Use the repository's impeccable skill for visual implementation and review. Its Operate mode applies to capture and settings. The explicit user brief fixes the visual system and toolbar direction. Further approval rounds are not required for implementation.

Browser proof covers component behavior. Separate operating-system proof covers the Tauri window, background lifecycle, permissions, and capture. The [delivery plan](docs/design/delivery-plan.md) records remaining work. The migration is not complete.
