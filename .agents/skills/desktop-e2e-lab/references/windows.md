# Windows lane

Use the allowlisted official Windows 11 Enterprise Evaluation x64 ISO and Microsoft-published SHA-256 through `SNIPTALE_E2E_WINDOWS_ISO` and `SNIPTALE_E2E_WINDOWS_ISO_SHA256`. Never use host Windows as the guest.

The golden provides a local-console autologin session, key-based OpenSSH, MSVC Build Tools, Windows SDK, Rust and the MSVC target, .NET, WiX v4, Chrome, and Edge. Builds use guest NTFS. Runtime proves denied external TCP egress before elevated MSI work.

Current-build acceptance requires release EXE/MSI/hash evidence and covers baseline install, major upgrade, destructive repair, uninstall, resident/tray/settings behavior, resize/DPI/theme, Chrome and Edge Native Messaging, Manual/Extension authority, reconnect/recovery, screenshot/region capture, and synthetic video/system/microphone recording. Separate non-silent system-loopback and microphone MP4 cases are required.

`test windows-11 --artifact <current-0.1.0-profile.msi>` also requires the matching `0.0.1-<profile>.msi`, baseline and current executables, and `native-host.json`. Missing companions fail before boot; the installed MSI is then checked for its embedded legal payload.

Drive UI through the guest console session. Save bounded screenshots and media metadata using synthetic fixtures. Verify installed executable, manifest, registration, legal payload, and hashes. Uninstall must remove registration, autostart, binaries, and private disposable state while preserving user-published Manual assets.
