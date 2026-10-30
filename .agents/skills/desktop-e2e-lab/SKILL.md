---
name: desktop-e2e-lab
description: Build, install, exercise, and diagnose Sniptale Windows or future Linux desktop artifacts in isolated QEMU/KVM guests. Use for package builds, VM E2E, desktop runtime defects, clean-snapshot reproduction, package lifecycle proof, and evidence collection.
---

# Desktop E2E lab

Operate from the repository root through `tooling/desktop/e2e-lab`. Do not install Sniptale on host Windows or substitute host-side builds for isolated acceptance evidence.

## Workflow

1. Run `tooling/desktop/e2e-lab doctor`; stop on any required failure and do not replace KVM with TCG for acceptance.
2. Run `bootstrap` only for missing host tools, inspect `status --json`, and provision a missing lane with `provision <lane>`.
3. Start a clean overlay with `run --lane <lane> --build current`; use `test <lane> --artifact <path>` only with every typed companion artifact required by the lane reference.
4. Require source, dirty-state, artifact, OS, snapshot, and tool fingerprints in `receipt.json`. Missing requested cases, skips, hash mismatch, incomplete JUnit, or absent evidence fail the run.
5. Run `collect <run-id>`, require `verified: true`, and inspect the receipt, JUnit, environment, artifact manifest, bounded logs, screenshots, and recordings.
6. Confirm teardown with `status --json`; use `reset <lane>` when a VM remains. Never reuse an acceptance overlay.

For iterative Windows development, use the separate `session` commands described in [Windows](references/windows.md). A development session retains its OS, guest compilation cache, installation, and browser profiles across changes. Its diagnostic receipts cannot replace clean acceptance. Use controller recovery for failed guest processes while the VM generation and boot remain valid. Close an expired, rebooted, or completed session through the controller.

Use `tooling/qa/wrappers/desktop-e2e-lab.sh --lane <lane|all>` for the standard lane. Report run ID, exact result matrix, fingerprints, forced teardown state, and evidence path. `rerun-failed <run-id>` is diagnostic only; acceptance requires a subsequent complete clean run.

## References

- Read [topology](references/topology.md) for ownership, trust, state, cache, concurrency, or cleanup changes.
- Read [Windows](references/windows.md) for MSI, tray/settings, Native Messaging, capture, recording, audio, or recovery.
- Read [Linux](references/linux.md) for Wayland/X11, portal, AppIndicator, PipeWire, browser, AT-SPI, `.deb`, or portable artifacts.
- Read [troubleshooting](references/troubleshooting.md) only after a doctor, provision, boot, guest, teardown, or evidence failure.

## Safety

Keep management endpoints on loopback or Unix sockets. Transfer bounded source bundles and explicit artifacts only; never mount host homes, `/mnt/c`, UNC paths, `.git`, secrets, user media, cache, or VM state into a guest. Use synthetic media. Delete only resolved descendants of `.tmp/qa/desktop-e2e-lab` or `.artifacts/desktop-e2e-lab` through controller commands.
