# Troubleshooting

## Host and provisioning

If KVM access fails, correct group or device ACL through an authorized host session and start a new WSL login; do not use TCG for acceptance. Use `bootstrap` for missing lab-local tools. Inspect only bounded tails of provisioning/runtime QEMU and serial logs. A checksum failure requires discarding the partial input; never bless the observed hash. Destroy and reprovision only the affected lane when golden identity is stale or corrupt.

## Guest failures

Collect first and require `verified: true`, then correlate the bounded lane log, strict guest result, screenshots, and artifact manifest. Missing cases and skips are failures. Diagnose session/D-Bus/PipeWire layers before desktop symptoms; on Windows separate MSI, resident, Explorer tray, settings window, browser Native Messaging, and media-owner failures.

Use `rerun-failed` only after a verified full-run failure, then rerun the complete clean matrix. Treat build-cache corruption as a miss. Do not edit receipts.

## Teardown

Use `reset <lane>` after timeout or transport loss and confirm no matching QEMU process or runtime overlay remains. Preserve incomplete receipts as failed evidence and use controller `gc` instead of manual recursive deletion.
