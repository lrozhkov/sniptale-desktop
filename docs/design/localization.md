# UI localization

Use [Fluent](https://projectfluent.org/) for shared UI text. The [official Rust implementation](https://github.com/projectfluent/fluent-rs) provides message parsing, parameters, plural rules, and bidirectional isolation. `src/tray/localization.rs` owns retained native formatting. `ui/src/localization/` owns web formatting through the official Fluent JavaScript implementation. `resources/locales/**` owns translations.

## Message contracts

Use typed message identifiers for labels, commands, instructions, validation, feedback, accessible names, and tooltips. Use typed argument variants for dynamic messages. Keep complete sentences in Fluent resources. Do not assemble translated sentences from fragments. Preserve translator comments and variable names.

Never translate protocol identifiers, command IDs, diagnostic categories, machine JSON, persisted values, user paths, device names, or user-entered text. Format those values only for display. Never parse localized output to recover application state. Do not log arguments or rendered messages.

The initial catalogs are English and Russian. Add a locale resource and register its tag and native name in the locale table. Components must not contain language-specific branches. Validate every shipped message with its expected arguments before enabling a language.

## Selection and fallback

Use Fluent language negotiation for requested tags, script variants, regions, and the English fallback. Web selection negotiates the requested language list. The retained native adapter reads the Windows preferred UI-language list. Bound and validate native buffer length, count, termination, UTF-16, and language tags. Failed lookup selects English without changing user preferences.

Keep English available independently of the selected language. Missing or invalid translations use the English message. Invalid English resources prevent UI initialization. The Fluent bundle's locale list does not provide message fallback by itself. The repository formatter performs that fallback explicitly.

Keep language preferences separate from capture authority. Language changes must preserve drafts, custom values, accepted selection, pending work, and valid focus and scroll context. The sandbox changes language only in memory. Product persistence belongs to `settings/**`.

## Layout and accessibility

Measure rendered text rather than character counts. Allow longer translations and independent text scaling. Keep essential controls reachable without horizontal form scrolling. Use logical start/end geometry for future right-to-left layouts. Preserve direction isolation around embedded values. Native control names and relevant accessibility properties follow the visible language.

Include expanded-text and right-to-left stress specimens in sandbox verification. A stress specimen does not establish translation quality or certify a production language. Validate actual shipped languages separately.

## Required proof

- Parse every catalog with the actual Fluent parser. Reject duplicate or malformed messages.
- Validate typed keys and arguments for every shipped language.
- Cover region fallback, unsupported tags, missing messages, malformed resources, and invalid system-language buffers.
- Cover Russian singular, few, many, and teen plural cases.
- Verify language changes, long labels, text scaling, focus, and accessibility in the shared browser sandbox. Check actual system dialogs separately.

[Delivery plan](delivery-plan.md) owns implementation status and Windows evidence. [Testing policy](../engineering/testing-policy.md) owns general proof requirements.
