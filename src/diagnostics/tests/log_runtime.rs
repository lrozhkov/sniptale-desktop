use std::{error::Error, fs};

use crate::diagnostics::log::{
    record::LogLevel,
    runtime::{ProcessLogEvent, start_process_logger, start_process_logger_for_mode_with_options},
    setup::LogSetupInputs,
};

#[test]
fn process_logger_writes_startup_record_to_managed_file() -> Result<(), Box<dyn Error>> {
    let temp = tempfile::tempdir()?;

    let logger = start_process_logger(&inputs(temp.path()), 1_000, 77)?;
    let active_file_name = logger.outcome().active_file_name.clone();
    drop(logger);

    let log_path = temp
        .path()
        .join("Sniptale")
        .join("NativeAgent")
        .join("logs")
        .join(active_file_name);
    let content = fs::read_to_string(log_path)?;

    assert!(content.contains("\"processMode\":\"native-host\""));
    assert!(content.contains("\"operation\":\"startup\""));
    assert!(content.contains("\"phase\":\"logging\""));
    assert!(content.contains("process logging initialized"));
    assert!(!content.contains(temp.path().to_string_lossy().as_ref()));
    Ok(())
}

#[test]
fn process_logger_redacts_events_before_writing() -> Result<(), Box<dyn Error>> {
    let temp = tempfile::tempdir()?;

    let mut logger = start_process_logger(&inputs(temp.path()), 1_000, 77)?;
    logger.write_event(
        1_001,
        ProcessLogEvent {
            level: LogLevel::Info,
            target: "sniptale_native_agent::transfer",
            operation: "transfer",
            operation_id: Some("capture-1"),
            phase: "chunk",
            message: "https://example.invalid/private data:image/png;base64,AAAA_SECRET_MEDIA_BYTES",
            duration_ms: Some(4),
            error: None,
        },
    )?;
    let active_file_name = logger.outcome().active_file_name.clone();
    drop(logger);

    let content = fs::read_to_string(
        temp.path()
            .join("Sniptale")
            .join("NativeAgent")
            .join("logs")
            .join(active_file_name),
    )?;

    assert!(content.contains("[redacted:base64-media]"));
    assert!(!content.contains("example.invalid"));
    assert!(!content.contains("AAAA_SECRET_MEDIA_BYTES"));
    assert!(content.contains("\"operation\":\"transfer\""));
    assert!(content.contains("\"operationId\":\"[redacted:media-id kind=screenshot]\""));
    assert!(!content.contains("capture-1"));
    Ok(())
}

#[test]
fn process_logger_now_paths_redact_sink_events() -> Result<(), Box<dyn Error>> {
    let temp = tempfile::tempdir()?;
    let log_dir = temp.path().join("support-logs");

    let mut logger = start_process_logger_for_mode_with_options(
        "collect-diagnostics",
        Some("trace".to_owned()),
        Some(log_dir.clone()),
    )?;
    logger.write_event_now(ProcessLogEvent {
        level: LogLevel::Trace,
        target: "sniptale_native_agent::diagnostics",
        operation: "support",
        operation_id: Some("recording-secret-9"),
        phase: "bundle",
        message: "chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/",
        duration_ms: None,
        error: None,
    })?;
    let sink = logger.sink();
    sink.write_event_now(ProcessLogEvent {
        level: LogLevel::Warn,
        target: "sniptale_native_agent::diagnostics",
        operation: "support",
        operation_id: Some("capture-secret-9"),
        phase: "bundle",
        message: "Authorization: Bearer secret",
        duration_ms: None,
        error: None,
    })?;
    logger.write_startup_recording_recovery_summary_now(1, 1, 0, 0)?;
    logger.write_startup_screenshot_recovery_summary_now(1, 1, 0, 0)?;
    drop(logger);

    let entries = fs::read_dir(&log_dir)?.collect::<Result<Vec<_>, _>>()?;
    assert_eq!(entries.len(), 1);
    let content = fs::read_to_string(entries[0].path())?;

    assert!(content.contains("[redacted:extension-origin]"));
    assert!(content.contains("[redacted:auth-header]"));
    assert!(content.contains("[redacted:media-id kind=recording]"));
    assert!(content.contains("[redacted:media-id kind=screenshot]"));
    assert!(!content.contains("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"));
    assert!(!content.contains("recording-secret-9"));
    assert!(!content.contains("capture-secret-9"));
    assert!(!content.contains("Bearer secret"));
    Ok(())
}

#[test]
fn process_logger_writes_redacted_startup_recovery_summary() -> Result<(), Box<dyn Error>> {
    let temp = tempfile::tempdir()?;

    let mut logger = start_process_logger(&inputs(temp.path()), 1_000, 77)?;
    logger.write_startup_recording_recovery_summary(1_001, 2, 1, 1, 3)?;
    let active_file_name = logger.outcome().active_file_name.clone();
    drop(logger);

    let content = fs::read_to_string(
        temp.path()
            .join("Sniptale")
            .join("NativeAgent")
            .join("logs")
            .join(active_file_name),
    )?;

    assert!(content.contains("\"operation\":\"startup\""));
    assert!(content.contains("\"phase\":\"recovery-scan\""));
    assert!(content.contains("pendingRecordings=2"));
    assert!(content.contains("keepWriting=1"));
    assert!(content.contains("retainedRejected=1"));
    assert!(content.contains("deleted=3"));
    assert!(!content.contains(temp.path().to_string_lossy().as_ref()));
    Ok(())
}

#[test]
fn process_logger_writes_redacted_startup_screenshot_recovery_summary() -> Result<(), Box<dyn Error>>
{
    let temp = tempfile::tempdir()?;

    let mut logger = start_process_logger(&inputs(temp.path()), 1_000, 77)?;
    logger.write_startup_screenshot_recovery_summary(1_001, 2, 1, 1, 3)?;
    let active_file_name = logger.outcome().active_file_name.clone();
    drop(logger);

    let content = fs::read_to_string(
        temp.path()
            .join("Sniptale")
            .join("NativeAgent")
            .join("logs")
            .join(active_file_name),
    )?;

    assert!(content.contains("\"operation\":\"startup\""));
    assert!(content.contains("\"phase\":\"screenshot-recovery-scan\""));
    assert!(content.contains("pendingScreenshots=2"));
    assert!(content.contains("keepWriting=1"));
    assert!(content.contains("retainedRejected=1"));
    assert!(content.contains("deleted=3"));
    assert!(!content.contains(temp.path().to_string_lossy().as_ref()));
    Ok(())
}

fn inputs(root: &std::path::Path) -> LogSetupInputs {
    LogSetupInputs {
        process_mode: "native-host",
        local_app_data_dir: Some(root.to_path_buf()),
        temp_dir: root.join("temp"),
        env_level: None,
        cli_level: None,
        cli_log_dir: None,
        debug_build: false,
    }
}
