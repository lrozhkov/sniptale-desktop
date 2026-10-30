# Windows lane

Use the allowlisted official Windows 11 Enterprise Evaluation x64 ISO and Microsoft-published SHA-256 through `SNIPTALE_E2E_WINDOWS_ISO` and `SNIPTALE_E2E_WINDOWS_ISO_SHA256`. Never use host Windows as the guest.

The golden provides a local-console autologin session, key-based OpenSSH, MSVC Build Tools, Windows SDK, Rust and the MSVC target, .NET, Chrome, and Edge. Rust, WiX, and SDK versions come from the central tool catalog and verified lock; preparation refreshes them without rebuilding the OS image. Builds use guest NTFS. Runtime proves denied external TCP egress before elevated MSI work. Before an expensive operation, the controller requires sufficient Windows license validity for its time budget. Expired Evaluation is a setup failure; refresh the isolated golden through supported Microsoft activation or installation procedures. Never reset activation timers automatically or disable licensing services. A consumable rearm requires explicit user authorization.

Current-build acceptance requires release EXE/MSI/hash evidence and covers baseline install, major upgrade, destructive repair, uninstall, resident/tray/settings behavior, resize/DPI/theme, Chrome and Edge Native Messaging, Manual/Extension authority, reconnect/recovery, screenshot/region capture, and synthetic video/system/microphone recording. Separate non-silent system-loopback and microphone MP4 cases are required.

`test windows-11 --artifact <current-0.1.0-profile.msi>` also requires the matching `0.0.1-<profile>.msi`, baseline and current executables, and `native-host.json`. Missing companions fail before boot; the installed MSI is then checked for its embedded legal payload.

Drive UI through the guest console session. Save bounded screenshots and media metadata using synthetic fixtures. Verify installed executable, manifest, registration, legal payload, and hashes. Uninstall must remove registration, autostart, binaries, and private disposable state while preserving user-published Manual assets.

## Iterative development sessions

Use a session when a feature requires repeated OS and browser checks after code changes. Set `SNIPTALE_EXTENSION_REPOSITORY` to the local Sniptale extension checkout. Start with doctor and status, then use the following commands from the desktop repository:

```bash
tooling/desktop/e2e-lab session open
tooling/desktop/e2e-lab session build --profile dev
tooling/desktop/e2e-lab session test --cases capture
tooling/desktop/e2e-lab session build --profile dev
tooling/desktop/e2e-lab session test \
  --extension-source "$SNIPTALE_EXTENSION_REPOSITORY" \
  --extension-artifact "$SNIPTALE_EXTENSION_REPOSITORY/.tmp/e2e-builds/test" \
  --cases real-extension
tooling/desktop/e2e-lab session status
tooling/desktop/e2e-lab session recover
tooling/desktop/e2e-lab session collect <session-id> <iteration-id>
tooling/desktop/e2e-lab session close --save
```

Omit `--cases` for the complete development runtime regression. Supply both extension arguments to include the real extension scenario. The controller invokes the extension owner's pinned npm test build when its source or artifact fingerprint changes; a successful build is reused only while both fingerprints match. Artifact runtime bytes remain unchanged; the laboratory manifest explicitly selects the dev host channel, allows nativeMessaging, and uses the isolated allowlisted identity. The scenario drives the product runtime in headful Chrome and Edge, checks lease and settings, decodes and hashes received native PNG media, and reconnects after resident restart. Browser profiles persist for subsequent iterations; product data in this guest is synthetic.

Development runtime checks update the installed application and retain its profile. They exclude MSI baseline, upgrade, destructive repair, and uninstall cases; the complete clean acceptance lane still owns those cases. `development-receipt.json` explicitly records `release_admissible: false`. Ordinary release receipt collection rejects this mode. A failed or interrupted guest operation requires `session recover` before more work. Recovery stops laboratory tasks and product, browser, and compiler processes while retaining the OS, compiler target, and browser profiles; an active installer transaction requires a fresh session. Host input-validation failures do not invalidate the guest. A dependency-lock change requires `session build`: the controller revalidates release ages, refreshes verified offline sources, and retains the OS and compilation target. An interrupted dependency refresh invalidates its vendor fingerprint and requires recovery plus a successful build. A guest reboot requires a fresh session. Lifetime and iteration bounds are owned by `work_sessions.py`; storage retention uses controller GC and lab configuration. Close the session when the task ends. Use `session close --save` after a successful iteration to shut down Windows cleanly and retain a bounded diagnostic work image containing compiler outputs, prepared tools, and synthetic profiles. If cache creation fails after clean shutdown, the controller retains the disk and an identity-bound shutdown marker; retry `session close --save` after resolving the reported budget or tool failure. A retry rejects a changed disk, changed VM generation, or running VM. A later `session open` verifies its hash, age, golden backing, and non-release mode before copying it into a separate writable overlay. Expiration and ordinary close discard the active overlay; clean acceptance never loads this cache. Cache bounds are owned by `session_cache.py` and the lab configuration.

For required Windows smoke, use the guest's synchronized source at `C:\SniptaleLab\work` after a successful current-source session build and a fresh host checkpoint. Run the tracked `tooling/release/windows/smoke/windows-smoke.ps1` in the guest console session through an interactive scheduled task with Limited privileges and normal task priority (`Settings.Priority = 4`), inside the isolated guest with `-SkipCheckpoint -Channel stable -DevLocalTrust`. Set `RUSTUP_TOOLCHAIN=sniptale-lab`, `CARGO_TARGET_DIR=C:\SniptaleLab\warm-target`, `CARGO_BUILD_TARGET=x86_64-pc-windows-msvc`, and `SNIPTALE_WINDOWS_TOOLS_CACHE=C:\SniptaleLab\build-tools` in that guest process. The shared initializer prepares pinned portable Python from the verified transferred package. Collect its evidence before the next source synchronization. Smoke changes the installed channel; run a successful development build/test iteration afterwards to restore the development package and prove its installed hash. Do not run this procedure on host Windows.

Iteration timing includes preflight and total wall time. Total contains the phase durations; do not add it to them. Session expiry waits for the bounded in-flight command; an iteration that ends after expiry is rejected, and controller teardown follows after the operation releases its lock.
