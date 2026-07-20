use std::error::Error;

use serde::Deserialize;

use super::super::log::record::{
    LogErrorFields, LogLevel, StructuredLogContext, build_structured_log_record,
    serialize_log_record_json_line,
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SerializedRecord {
    timestamp_epoch_ms: u64,
    level: String,
    target: String,
    app_version: String,
    protocol_version: u16,
    settings_schema_version: u16,
    app_cache_schema_version: u16,
    platform: SerializedPlatform,
    process_mode: String,
    process_role: String,
    process_id: u32,
    connection_id: String,
    controller_lease_id: String,
    operation: String,
    operation_id: String,
    phase: String,
    duration_ms: u64,
    error: SerializedError,
    message: String,
}

#[derive(Deserialize)]
struct SerializedPlatform {
    kind: String,
}

#[derive(Deserialize)]
struct SerializedError {
    code: String,
    recoverable: bool,
}

#[test]
fn structured_log_record_serializes_required_fields_and_redacts_message()
-> Result<(), Box<dyn Error>> {
    let record = build_structured_log_record(
        12_345,
        LogLevel::Warn,
        "sniptale_native_agent::ipc",
        context(),
        "{\"type\":\"extension.screenshot.capture\",\"token\":\"secret-token\"}",
        Some(42),
        Some(LogErrorFields {
            code: "stale-controller-lease".to_owned(),
            recoverable: true,
        }),
    );

    let line = serialize_log_record_json_line(&record)?;
    let serialized: SerializedRecord = serde_json::from_str(&line)?;

    assert!(line.ends_with('\n'));
    assert_eq!(serialized.timestamp_epoch_ms, 12_345);
    assert_eq!(serialized.level, "warn");
    assert_eq!(serialized.target, "sniptale_native_agent::ipc");
    assert_eq!(serialized.app_version, "0.1.0");
    assert_eq!(serialized.protocol_version, 1);
    assert_eq!(serialized.settings_schema_version, 1);
    assert_eq!(serialized.app_cache_schema_version, 1);
    assert_eq!(serialized.platform.kind, "windows");
    assert_eq!(serialized.process_mode, "native-host");
    assert_eq!(serialized.process_role, "native-host");
    assert_eq!(serialized.process_id, 42);
    assert_eq!(serialized.connection_id, "[redacted:connection-id]");
    assert_eq!(
        serialized.controller_lease_id,
        "[redacted:controller-lease-id]"
    );
    assert_eq!(serialized.operation, "screenshot");
    assert_eq!(serialized.operation_id, "[redacted:operation-id]");
    assert_eq!(serialized.phase, "lease-validation");
    assert_eq!(serialized.duration_ms, 42);
    assert_eq!(serialized.error.code, "stale-controller-lease");
    assert!(serialized.error.recoverable);
    assert!(
        serialized
            .message
            .contains("message-type=extension.screenshot.capture")
    );
    assert!(!line.contains("secret-token"));
    assert!(!line.contains("{\"type\""));
    assert!(!line.contains("connection-1"));
    assert!(!line.contains("command-1"));
    Ok(())
}

#[test]
fn structured_log_record_redacts_urls_and_media_bytes() -> Result<(), Box<dyn Error>> {
    let url = build_structured_log_record(
        1,
        LogLevel::Info,
        "sniptale_native_agent::transfer",
        context(),
        "https://example.invalid/private",
        None,
        None,
    );
    let media = build_structured_log_record(
        1,
        LogLevel::Debug,
        "sniptale_native_agent::transfer",
        context(),
        "data:image/png;base64,AAAA_SECRET_MEDIA_BYTES",
        None,
        None,
    );

    let url_line = serialize_log_record_json_line(&url)?;
    let media_line = serialize_log_record_json_line(&media)?;

    assert!(url_line.contains("[redacted:url]"));
    assert!(!url_line.contains("example.invalid"));
    assert!(media_line.contains("[redacted:base64-media]"));
    assert!(!media_line.contains("AAAA_SECRET_MEDIA_BYTES"));
    Ok(())
}

fn context() -> StructuredLogContext {
    StructuredLogContext {
        app_version: "0.1.0".to_owned(),
        protocol_version: 1,
        settings_schema_version: 1,
        app_cache_schema_version: 1,
        platform_kind: "windows".to_owned(),
        process_mode: "native-host".to_owned(),
        process_id: 42,
        connection_id: Some("connection-1".to_owned()),
        controller_lease_id: Some("lease:connection-1".to_owned()),
        operation: "screenshot",
        operation_id: Some("command-1".to_owned()),
        phase: "lease-validation",
    }
}
