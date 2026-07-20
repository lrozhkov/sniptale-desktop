# Sniptale Desktop

**Capture beyond the browser. Keep the workflow local.**

Sniptale Desktop is a native companion for Sniptale and a standalone Windows capture application. It adds operating-system screenshot, screen-recording, audio, tray, and global-hotkey capabilities that a browser extension cannot reliably own by itself.

The application can work independently in Manual mode, saving user-requested PNG and MP4 files to a selected local folder, or connect to a compatible Chromium extension through Native Messaging and return captured media to the browser workspace. It does not require a Sniptale account or hosted Sniptale backend.

> **Experimental preview.** The Windows implementation is functional and has deterministic architecture, security, and packaging checks, but it still needs broader product validation, compatibility testing, performance work, signed-release evidence, and real-world use before it should be treated as a stable production application. Do not rely on the current build as the only copy of important media.

## Current status

| Area | Status |
| --- | --- |
| Windows | Experimental implementation; the current release target is `x86_64-pc-windows-msvc` |
| Manual mode | Experimental standalone tray application with local PNG/MP4 publication |
| Browser integration | Experimental Chromium Native Messaging integration; requires a compatible extension build and matching extension origin |
| Linux | Planned; no working Linux product build or package is included yet |
| macOS | The owner and adapter boundaries leave room for a future port, but a macOS implementation is not currently planned |

Windows is the only implemented desktop platform today. The protocol and state contracts are kept platform-neutral where practical, while Windows APIs remain behind owner-local adapters; this is the portability boundary for planned Linux work, not a claim that the current code already runs on Linux or macOS.

## Build from source

### Requirements

- Windows 10 or Windows 11 on x64.
- [Rust](https://www.rust-lang.org/tools/install) `1.96` or newer installed through `rustup`; `rust-toolchain.toml` selects the stable toolchain and the `x86_64-pc-windows-msvc` target.
- Visual Studio 2022 Build Tools with **Desktop development with C++**, the x64 MSVC toolchain, and a Windows 10 or Windows 11 SDK.
- Git.

Open **Developer PowerShell for VS 2022** in the repository root and build an unsigned development executable:

```powershell
$env:SNIPTALE_INSTALL_CHANNEL = "dev"
$env:SNIPTALE_WINDOWS_RESOURCES = "require"
cargo build --locked --release --target x86_64-pc-windows-msvc --bin sniptale-native-agent --all-features
```

The executable is written to:

```text
target\x86_64-pc-windows-msvc\release\sniptale-native-agent.exe
```

Start it directly to open the resident tray application:

```powershell
.\target\x86_64-pc-windows-msvc\release\sniptale-native-agent.exe
```

Open the tray settings, switch to **Manual**, and select a local destination before capturing. A direct source build is unsigned and does not install the Native Messaging manifest, autostart entry, or MSI registration.

To compile a binary for later extension packaging and registration, set the exact Chromium extension origin before the Cargo command:

```powershell
$env:SNIPTALE_ALLOWED_EXTENSION_ORIGIN = "chrome-extension://<32-character-extension-id>/"
```

The extension origin is compiled into the channel-specific install identity. Rebuild after changing it. Installable MSI generation also requires WiX v4 and the repository release gates; see the [local OSS release guide](docs/oss/release.md) instead of invoking WiX directly.

## Why this project exists

Sniptale Desktop is part of the same practical experiment as the Sniptale browser extension: **can a person who is not a software developer create and continue maintaining a native application through coding agents?**

I do not have the skills to write this codebase by hand. My role has been the product role: define the direction, describe the desired behavior, set acceptance criteria, inspect results, make architecture and security trade-offs, and decide what is ready to keep or release.

Sniptale Desktop was developed entirely through Codex coding-agent sessions under that product direction. All project-authored application code, Windows integrations, tests, refactors, installer definitions, documentation, and quality and release tooling were produced or revised by Codex agents rather than written by me by hand.

The project was not created as one large generation prompt. Development has repeatedly followed a constrained review loop:

```text
product intent and acceptance criteria
  → bounded Codex implementation task
  → deterministic checks and tests
  → separate architecture or security review when risk justifies it
  → consolidated refactor
  → repeat
```

This is also the intended development model going forward: new functionality, maintenance, refactoring, tests, documentation, and release work will continue to be carried out through Codex agents under human product direction and acceptance.

Agent-driven implementation is not treated as proof that the result is correct. The repository therefore turns architecture, security, testing, licensing, and release expectations into executable checks wherever practical, while human decisions remain responsible for product intent, trade-offs, and release acceptance.

## What Sniptale Desktop can do

### Capture screenshots

Capture the current screen, active window, complete virtual desktop, or a selected region. Cursor inclusion is configurable, and completed screenshots are encoded as PNG.

### Record the desktop

Record a screen, active window, or selected region to H.264/AAC MP4. Available settings include cursor capture, 24/30/60 FPS, quality and bitrate controls, hardware or software encoding preference, maximum duration, microphone audio, system audio, or a mixed audio track.

### Stay available from the tray

The resident application exposes capture and recording actions through its tray menu and configurable global hotkeys. Recording can be paused, resumed, or stopped without returning to the source window.

### Work with or without the browser extension

Manual mode owns its local settings and publishes only user-requested PNG/MP4 files beneath the selected destination. ExtensionManaged mode accepts versioned settings and commands from the compatible extension, then transfers finalized media through a bounded, integrity-checked delivery path. The two authorities are mutually exclusive for an operation.

### Produce bounded diagnostics

Support commands report version, health, registration assessment, and redacted diagnostics without returning raw media or unrestricted runtime state. See [Diagnostics and support](docs/operations/diagnostics.md) for the supported commands.

## Architecture

Sniptale Desktop is implemented in Rust 2024. The Windows product uses direct Win32, Windows Graphics Capture, WIC, Media Foundation, WASAPI, and WiX adapters; it does not embed Electron, a browser-rendered desktop shell, or a general asynchronous runtime.

The installed executable remains named `sniptale-native-agent.exe` because the same binary can run in three bounded roles:

- a resident tray process that owns capture, recording, settings, local publication, and authenticated local IPC;
- a browser-launched Native Messaging bridge that validates the extension origin before connecting to the resident process;
- support commands for version, health, registration assessment, and diagnostic collection.

The resident runtime is one process with owner-specific, bounded worker lifecycles. Product state and behavior are split among named owners such as `capture`, `recording`, `audio`, `settings`, `transfer`, `install`, and `diagnostics`; cross-owner coordination uses narrow typed interfaces, while raw Windows handles and privileged APIs stay inside the adapter that acquired them.

Wire contracts are platform-neutral, versioned, size-bounded, and parsed at their receiving boundary. Native Messaging and resident IPC carry commands and control state; media delivery remains transfer-owned rather than becoming an unbounded message bus. See the [repository overview](docs/architecture/repository-overview.md), [module ownership](docs/architecture/module-ownership.md), and [architecture decisions](docs/README.md#architecture).

## Security and privacy

Sniptale Desktop is local-first, but local software is not automatically safe. The current design includes the following controls:

- screenshot and recording temp media is bounded, protected with per-media keys, and removed after acknowledgement, rejection, or retention expiry;
- the per-install resident-pipe secret is protected for the current Windows user, and clients must complete a live HMAC challenge before commands are accepted;
- extension origin, native consent, session limits, controller authority, freshness, and replay checks fail closed at the IPC boundary;
- media chunks have explicit size limits and integrity hashes, and media bytes never traverse the resident control pipe;
- logs, health output, and support bundles use typed categories and redaction rules instead of media, typed text, clipboard values, URLs, tokens, identifiers, raw payloads, or full paths;
- Manual mode does not collect recording interaction telemetry and publishes plaintext media only to the user-selected local destination.

These controls are engineering measures, not a security certification or independent penetration test. A compromised Windows user session remains outside the protection boundary, Manual output inherits the destination's ACL, backup, and sync behavior, and the current single-process runtime does not isolate a process-wide memory-safety or privileged-adapter failure. Stable public packages additionally require Authenticode and release-provenance evidence.

Read the [threat model](docs/security/threat-model.md), [data-handling rules](docs/security/data-handling.md), [IPC authorization design](docs/security/ipc-auth.md), and [install-trust policy](docs/security/install-trust.md) before depending on the application for sensitive workflows.

## Known limits

- Windows is the only implemented runtime and package target.
- The application is a capture companion, not a gallery, editor, OCR, transcription, AI, account, cloud-sync, or collaboration service.
- Gallery and editor workflows belong to the separate browser extension and are not included in this repository.
- Protected or unavailable operating-system sources may not be capturable, and audio/encoder availability depends on the Windows device and drivers.
- No automatic updater is included.
- Stable packaging still requires signed Windows artifacts and the manual acceptance evidence described in the release guide.

## Repository and quality model

- `src/**` contains product owners and owner-local tests.
- `contracts/**` contains executable wire, parser, limit, error, and redaction fixtures.
- `installer/**` and `resources/**` contain Windows packaging inputs.
- `tooling/**` contains canonical QA, release, audit, and desktop E2E entrypoints.
- [`docs/README.md`](docs/README.md) indexes the active architecture, engineering, security, operations, provenance, and release documentation.
- [`AGENTS.md`](AGENTS.md) defines the compact repository workflow for coding agents.

The repository uses formatting, linting, owner-boundary checks, negative parser and state-machine proof, coverage policy, dependency and license checks, Windows-target compilation, installed-runtime smoke tests, isolated desktop E2E, secret scanning, and release-evidence validation. These controls reduce risk; they do not replace real-user testing, performance profiling, accessibility review, adversarial testing, or independent security assessment.

All repository QA and release gates run locally. GitHub Actions workflows are intentionally absent, and a local tooling guard rejects adding hosted workflows so heavyweight checks cannot consume GitHub runner resources accidentally.

## License and contributions

Sniptale Desktop is licensed under the [MIT License](LICENSE). Third-party components retain their own terms; see the [third-party notices](THIRD_PARTY_NOTICES.md) and [OSS provenance](docs/oss/provenance.md).

External code contributions are not accepted at this time. Public bug reports and product proposals must not include sensitive data, and suspected vulnerabilities must not be disclosed in public issues. See [CONTRIBUTING.md](CONTRIBUTING.md).
