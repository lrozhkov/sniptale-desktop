//! Resident pipe accept/session result accounting.

use std::sync::{Arc, Mutex};

use super::super::{
    AgentPipeRejection, AgentPipeServerError, AgentPipeServerReport,
    logging::{AgentPipeServerLogSink, log_resident_pipe_event},
};
use super::report::{increment_completed, increment_failed};

pub(super) fn record_accept_rejection(
    report: &Arc<Mutex<AgentPipeServerReport>>,
    log: Option<&AgentPipeServerLogSink>,
    rejection: AgentPipeRejection,
) -> Result<(), AgentPipeServerError> {
    let failed = increment_failed(report)?;
    tracing::warn!(
        target: "sniptale_native_agent::resident_pipe",
        failed,
        code = rejection.code,
        "resident pipe rejected native-host session"
    );
    log_resident_pipe_event(
        log,
        rejection.phase,
        rejection.message,
        Some(rejection.code),
    );
    Ok(())
}

pub(super) fn record_session_result(
    report: &Arc<Mutex<AgentPipeServerReport>>,
    log: Option<&AgentPipeServerLogSink>,
    result: Result<(), crate::app::bridge::session::AgentPipeSessionError>,
) {
    let failed_session = result.is_err();
    // Release parser error storage before emitting status-only diagnostics.
    drop(result);
    if failed_session {
        if let Ok(failed) = increment_failed(report) {
            tracing::warn!(
                target: "sniptale_native_agent::resident_pipe",
                failed,
                "resident pipe native-host session failed"
            );
            log_resident_pipe_event(
                log,
                "session-failed",
                "resident pipe native-host session failed",
                Some("resident_pipe.session_failed"),
            );
        } else {
            tracing::warn!(target: "sniptale_native_agent::resident_pipe", "resident pipe report update failed after session failure");
        }
    } else if let Ok(completed) = increment_completed(report) {
        tracing::info!(target: "sniptale_native_agent::resident_pipe", completed, "resident pipe completed native-host session");
        log_resident_pipe_event(
            log,
            "session-completed",
            "resident pipe completed native-host session",
            None,
        );
    } else {
        tracing::warn!(target: "sniptale_native_agent::resident_pipe", "resident pipe report update failed");
    }
}

#[cfg(test)]
mod privacy_tests {
    use super::*;
    struct CapturedLog(Arc<Mutex<Vec<u8>>>);
    impl std::io::Write for CapturedLog {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0
                .lock()
                .map_err(|_| std::io::Error::other("log capture unavailable"))?
                .extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    #[test]
    fn rejected_parser_input_is_absent_from_actual_session_failure_trace()
    -> Result<(), Box<dyn std::error::Error>> {
        let private = "private-session-never-log.invalid";
        let Err(error) = crate::protocol::parse_wire_message(
            br#"{"type":"https://private-session-never-log.invalid/fixture","protocolVersion":2}"#,
        ) else {
            return Err("malformed input unexpectedly accepted".into());
        };
        assert!(format!("{error:?}").contains(private));
        let bytes = Arc::new(Mutex::new(Vec::new()));
        let output = Arc::clone(&bytes);
        let subscriber = tracing_subscriber::fmt()
            .without_time()
            .with_ansi(false)
            .with_max_level(tracing::Level::WARN)
            .with_writer(move || CapturedLog(Arc::clone(&output)))
            .finish();
        let report = Arc::new(Mutex::new(AgentPipeServerReport::default()));
        tracing::subscriber::with_default(subscriber, || {
            record_session_result(
                &report,
                None,
                Err(crate::app::bridge::session::AgentPipeSessionError::Protocol(error)),
            );
        });
        let output = String::from_utf8(bytes.lock().map_err(|_| "log capture")?.clone())?;
        assert!(output.contains("resident pipe native-host session failed"));
        assert!(!output.contains(private));
        assert!(!output.contains("https://"));
        assert_eq!(report.lock().map_err(|_| "report capture")?.failed, 1);
        Ok(())
    }
}
