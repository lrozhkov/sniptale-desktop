# Repository overview

Sniptale Desktop is the Windows-native Sniptale process. The installed `sniptale-native-agent.exe` runs as a resident tray application, a Chromium Native Messaging bridge, or a bounded support command.

## Runtime modes

- No mode flag starts the resident tray process, owner workers, Manual-mode UI, local publication, and authenticated resident pipe when extension ingress is allowed.
- `--native-host <extension-origin>` runs the browser-launched stdio bridge. Browser origin validation precedes resident-pipe connection.
- `--version-json`, `--doctor-json`, `--repair-registration`, and `--collect-diagnostics` emit bounded support output. Despite its historical name, `--repair-registration` reports the current repair assessment and does not mutate registration.

`src/app/**` owns mode dispatch and lifecycle only. Product ownership is defined in [module ownership](module-ownership.md).

## Technology boundary

The release target is Windows MSVC. Product UI and OS integration use Rust with direct Win32, Windows Graphics Capture, WIC, Media Foundation, WASAPI, and WiX adapters; no browser-rendered desktop shell or general async runtime is part of the product.

## Authoritative inputs

- `src/**` and owner-local tests define runtime behavior.
- `contracts/**` defines executable wire fixtures, parser limits, error catalogs, and redaction corpus.
- `tooling/configs/qa/**`, `deny.toml`, and wrapper code define machine policy and gate behavior.
- `installer/**` and `resources/**` define Windows packaging inputs.
- `docs/architecture/adr/**` defines accepted trade-offs that code cannot explain.

Ignored `.tmp/**` and `.artifacts/**` contain generated evidence, caches, VM inputs, or local task material and are never repository authority.

## Product boundary

The native process does not own accounts, cloud sync, a media gallery, an editor, OCR, transcription, or AI summaries. Manual mode may persist a local profile and publish user-selected PNG/MP4 files only under [ADR 0005](adr/0005-dual-settings-authority-and-local-asset-delivery.md).
