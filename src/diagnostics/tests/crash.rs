use super::super::log::{
    crash::{CrashContext, build_crash_log_record, build_exception_log_record},
    record::serialize_log_record_json_line,
};

#[test]
fn crash_log_record_keeps_phase_and_redacts_payload() -> Result<(), Box<dyn std::error::Error>> {
    let record = build_crash_log_record(
        44,
        &CrashContext::with_phase(
            "native-host",
            "recording",
            Some("recording-1".to_owned()),
            "finalizing",
        ),
        "{\"type\":\"extension.recording.start\",\"token\":\"secret-token\"}",
    );
    let line = serialize_log_record_json_line(&record)?;
    let value: serde_json::Value = serde_json::from_str(&line)?;

    assert_eq!(value["level"], "error");
    assert_eq!(value["target"], "sniptale_native_agent::panic");
    assert_eq!(value["processMode"], "native-host");
    assert_eq!(value["operation"], "recording");
    assert_eq!(value["operationId"], "[redacted:media-id kind=recording]");
    assert_eq!(value["phase"], "finalizing");
    assert_eq!(value["error"]["code"], "unhandled-panic");
    assert_eq!(value["error"]["recoverable"], false);
    assert!(
        value["message"]
            .as_str()
            .is_some_and(|message| message.contains("message-type=extension.recording.start"))
    );
    assert!(!line.contains("secret-token"));
    assert!(!line.contains("{\"type\""));
    assert!(!line.contains("recording-1"));
    Ok(())
}

#[test]
fn crash_log_record_redacts_media_and_paths() -> Result<(), Box<dyn std::error::Error>> {
    let media = build_crash_log_record(
        45,
        &CrashContext::for_mode("agent"),
        "data:video/mp4;base64,AAAA_SECRET_MEDIA_BYTES",
    );
    let path = build_crash_log_record(
        46,
        &CrashContext::for_mode("agent"),
        "panic at C:\\Users\\Alice\\Private\\recording.mp4",
    );

    let media_line = serialize_log_record_json_line(&media)?;
    let path_line = serialize_log_record_json_line(&path)?;

    assert!(media_line.contains("[redacted:base64-media]"));
    assert!(!media_line.contains("AAAA_SECRET_MEDIA_BYTES"));
    assert!(path_line.contains("[path-category:local]"));
    assert!(!path_line.contains("C:\\Users\\Alice"));
    Ok(())
}

#[test]
fn exception_log_record_keeps_phase_and_omits_addresses() -> Result<(), Box<dyn std::error::Error>>
{
    let record = build_exception_log_record(
        47,
        &CrashContext::with_phase("agent", "recording", Some("rec-7".to_owned()), "encoding"),
        Some(0xC000_0005),
    );
    let line = serialize_log_record_json_line(&record)?;
    let value: serde_json::Value = serde_json::from_str(&line)?;

    assert_eq!(value["level"], "error");
    assert_eq!(value["target"], "sniptale_native_agent::exception");
    assert_eq!(value["processMode"], "agent");
    assert_eq!(value["operation"], "recording");
    assert_eq!(value["operationId"], "[redacted:media-id kind=recording]");
    assert_eq!(value["phase"], "encoding");
    assert_eq!(value["error"]["code"], "unhandled-exception");
    assert_eq!(value["error"]["recoverable"], false);
    assert!(
        value["message"].as_str().is_some_and(|message| {
            message.contains("unhandled native exception code=0xC0000005")
        })
    );
    assert!(!line.contains("ExceptionAddress"));
    assert!(!line.contains("rec-7"));
    Ok(())
}
