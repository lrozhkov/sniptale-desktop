# Sniptale Desktop

Sniptale Desktop is a Windows capture application and a native companion for the Sniptale browser extension. It captures screenshots and records desktop video with optional audio. It runs without a Sniptale account or hosted backend.

**Experimental preview.** Keep separate backups of important media. Stable release requires the evidence in the [release procedure](docs/oss/release.md).

## Features

- Capture a screen, active window, virtual desktop, or selected region as PNG.
- Record a screen, window, or region as H.264/AAC MP4. Configure microphone audio, system audio, quality, frame rate, and duration.
- Control capture through the tray menu and global hotkeys. Pause, resume, or stop a recording.
- Save media to a selected local folder. Optionally deliver media to a compatible Chromium extension through Native Messaging.
- Inspect bounded, redacted [diagnostics](docs/operations/diagnostics.md).

Desktop owns capture settings and controls. The user selects a browser destination in Desktop. [ADR 0005](docs/architecture/adr/0005-dual-settings-authority-and-local-asset-delivery.md) defines delivery authority and recovery.

## Requirements

- Windows 10 or Windows 11 on x64.
- Rust through `rustup`. [rust-toolchain.toml](rust-toolchain.toml) selects the pinned toolchain and Windows target.
- Visual Studio 2022 Build Tools with Desktop development with C++, x64 MSVC, and a Windows SDK.
- Git.

The [tool catalog](tooling/configs/qa/tool-versions.json) owns exact managed versions. [QA tooling](docs/tooling/qa.md) explains tool installation.

## Build from source

Open Developer PowerShell for VS 2022 in the repository root. Build an unsigned development executable:

```powershell
$env:SNIPTALE_INSTALL_CHANNEL = "dev"
$env:SNIPTALE_WINDOWS_RESOURCES = "require"
cargo build --locked --release --target x86_64-pc-windows-msvc --bin sniptale-native-agent --all-features
```

Start the tray application:

```powershell
.\target\x86_64-pc-windows-msvc\release\sniptale-native-agent.exe
```

Open the tray settings. Select Manual mode and a local destination before capture. A direct source build does not install Native Messaging registration, autostart, or MSI state.

For extension packaging, set the exact extension origin before the Cargo command:

```powershell
$env:SNIPTALE_ALLOWED_EXTENSION_ORIGIN = "chrome-extension://<32-character-extension-id>/"
```

Rebuild after an origin change. Use the [release procedure](docs/oss/release.md) for installable packages and signing requirements.

## Limits and security

Windows is the only implemented product platform. Linux is planned. No macOS implementation is currently planned.

This application does not include a gallery, editor, OCR, transcription, AI features, accounts, cloud sync, or an automatic updater. Capture and audio availability depend on Windows, source protection, hardware, and drivers.

The application uses protected temporary media, authenticated local IPC, bounded transfers, and redacted diagnostics. These controls do not provide a security certification. A compromised Windows user session remains outside the protection boundary. Local output inherits the destination's ACL, backup, and sync behavior.

Read the [threat model](docs/security/threat-model.md) before sensitive use. [Data handling](docs/security/data-handling.md) defines storage and retention.

## Development and documentation

The product uses Rust with owner-local Windows adapters. The [repository overview](docs/architecture/repository-overview.md) explains runtime modes. The [owner map](docs/architecture/module-ownership.md) defines state authority and dependency direction.

[AGENTS.md](AGENTS.md) defines the implementation and review workflow. [QA tooling](docs/tooling/qa.md) defines canonical local and CI checks. The [documentation index](docs/README.md) lists all active documents.

## License and contributions

The project uses the [MIT License](LICENSE). Third-party components retain their own terms. Read [third-party notices](THIRD_PARTY_NOTICES.md) and [OSS provenance](docs/oss/provenance.md).

External code contributions are currently closed. Read [CONTRIBUTING.md](CONTRIBUTING.md) before a bug report or product proposal. Do not publish sensitive data or suspected vulnerabilities in public issues.
