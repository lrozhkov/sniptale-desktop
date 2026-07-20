# Diagnostics and support

## Commands

```text
sniptale-native-agent --version-json
sniptale-native-agent --doctor-json
sniptale-native-agent --repair-registration
sniptale-native-agent --collect-diagnostics
```

`--version-json` emits version and platform facts. `--doctor-json` emits health and repair categories. `--repair-registration` emits a registration repair assessment without changing the system. `--collect-diagnostics` writes a redacted bundle and reports only its category, leaf name, entry count, and repair state.

## Locations

Logs and diagnostics live below `%LOCALAPPDATA%\Sniptale\NativeAgent`. Temporary media, profiles, job stores, and destination paths must not be attached to reports. [Data handling](../security/data-handling.md) owns redaction and retention.

Production logging defaults to `info`; dev builds default to `debug`. Support modes accept `SNIPTALE_NATIVE_LOG`, `--log-level`, and `--log-dir`. Invalid levels and empty paths are rejected. Native Messaging stdout is reserved for framed protocol output.

## Triage

1. Run `--doctor-json` and inspect typed failed checks.
2. Run `--repair-registration` for manifest, signature, ACL, or autostart categories.
3. Run `--collect-diagnostics` for a bounded support bundle.
4. Correlate operation, phase, error code, and redacted identifier category in the latest managed log.
