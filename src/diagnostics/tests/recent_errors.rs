use std::{error::Error, fmt::Write as _, fs};

use serde_json::Value;

use super::super::recent_errors::recent_errors_json_from_log_dir;

#[test]
fn recent_errors_are_populated_from_logs_as_redacted_buckets() -> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    fs::write(
        directory.path().join("sniptale-native-2000-77-0.jsonl"),
        r#"{"timestampEpochMs":3600000,"level":"error","target":"sniptale_native_agent::transfer","operation":"transfer","phase":"chunk-read","error":{"code":"storage-failed","recoverable":true},"message":"C:\\Users\\Alice\\Private\\capture.mp4 capture-secret-9"}
{"timestampEpochMs":3600500,"level":"error","target":"sniptale_native_agent::transfer","operation":"transfer","phase":"chunk-read","error":{"code":"storage-failed","recoverable":true},"message":"token secret-token"}
{"timestampEpochMs":7200000,"level":"warn","target":"sniptale_native_agent::native_host","operation":"bridge","phase":"pipe-auth","error":{"code":"native_host.pipe_auth","recoverable":true},"message":"chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/"}
{"timestampEpochMs":7200000,"level":"info","target":"sniptale_native_agent::capture","operation":"capture","phase":"success","message":"no error"}
"#,
    )?;

    let value: Value = serde_json::from_str(&recent_errors_json_from_log_dir(directory.path())?)?;
    let recent_errors = value["recentErrors"]
        .as_array()
        .ok_or("missing recentErrors")?;

    assert_eq!(recent_errors.len(), 2);
    assert!(recent_errors.iter().any(|entry| {
        entry["owner"] == "transfer"
            && entry["category"] == "storage-failed"
            && entry["phase"] == "chunk-read"
            && entry["timestampBucket"] == "epoch-hour:1"
            && entry["recoverable"] == true
            && entry["count"] == 2
    }));
    assert!(recent_errors.iter().any(|entry| {
        entry["owner"] == "ipc"
            && entry["category"] == "native_host.pipe_auth"
            && entry["phase"] == "pipe-auth"
            && entry["timestampBucket"] == "epoch-hour:2"
            && entry["recoverable"] == true
            && entry["count"] == 1
    }));

    let output = value.to_string();
    assert!(!output.contains("C:\\Users\\Alice"));
    assert!(!output.contains("capture-secret-9"));
    assert!(!output.contains("secret-token"));
    assert!(!output.contains("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"));
    assert!(!output.contains("chrome-extension://"));
    Ok(())
}

#[test]
fn recent_errors_reject_label_shaped_sensitive_log_fields() -> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    fs::write(
        directory.path().join("sniptale-native-3000-77-0.jsonl"),
        r#"{"timestampEpochMs":3600000,"target":"sniptale_native_agent::diagnostics","phase":"password123","error":{"code":"copied-secret","recoverable":true},"message":"safe"}
{"timestampEpochMs":3600100,"target":"sniptale_native_agent::diagnostics","phase":"token","error":{"code":"secret-token","recoverable":true},"message":"safe"}
"#,
    )?;

    let value: Value = serde_json::from_str(&recent_errors_json_from_log_dir(directory.path())?)?;
    let recent_errors = value["recentErrors"]
        .as_array()
        .ok_or("missing recentErrors")?;

    assert_eq!(recent_errors.len(), 1);
    assert_eq!(recent_errors[0]["category"], "unknown");
    assert_eq!(recent_errors[0]["phase"], "unknown");
    assert_eq!(recent_errors[0]["count"], 2);
    let output = value.to_string();
    assert!(!output.contains("password123"));
    assert!(!output.contains("copied-secret"));
    assert!(!output.contains("secret-token"));
    Ok(())
}

#[test]
fn recent_errors_are_bounded_and_sanitize_untrusted_log_labels() -> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let mut content = String::new();
    let allowed = [
        ("storage-failed", "chunk-read"),
        ("native_host.pipe_auth", "pipe-auth"),
        ("resident_pipe.handshake_timeout", "handshake-timeout"),
        ("resident_pipe.session_failed", "session-failed"),
        (
            "resident_pipe.session_start_timeout",
            "session-start-timeout",
        ),
        (
            "resident_pipe.session_timeout_no_controller",
            "session-timeout-no-controller",
        ),
        (
            "resident_pipe.session_timeout_no_settings",
            "session-timeout-no-settings",
        ),
        ("telemetry.hook_drop", "hook-drop"),
        ("logger.file", "logger-startup"),
        ("agent.start_failed", "agent-run"),
    ];
    for index in 0..40 {
        let (category, phase) = allowed[index % allowed.len()];
        writeln!(
            content,
            r#"{{"timestampEpochMs":{},"target":"sniptale_native_agent::diagnostics","phase":"{phase}","error":{{"code":"{category}","recoverable":false}},"message":"safe"}}"#,
            3_600_000 * u64::try_from(index).unwrap_or_default()
        )?;
    }
    content.push_str(
        r#"{"timestampEpochMs":200000000,"target":"sniptale_native_agent::diagnostics","phase":"phase with token secret","error":{"code":"category with C:\\Users\\Alice","recoverable":true},"message":"safe"}
"#,
    );
    fs::write(
        directory.path().join("sniptale-native-1000-77-0.jsonl"),
        content,
    )?;

    let value: Value = serde_json::from_str(&recent_errors_json_from_log_dir(directory.path())?)?;
    let recent_errors = value["recentErrors"]
        .as_array()
        .ok_or("missing recentErrors")?;

    assert_eq!(recent_errors.len(), 32);
    assert!(recent_errors.iter().any(|entry| {
        entry["category"] == "unknown"
            && entry["phase"] == "unknown"
            && entry["owner"] == "diagnostics"
    }));
    let output = value.to_string();
    assert!(!output.contains("token"));
    assert!(!output.contains("C:\\Users\\Alice"));
    Ok(())
}
