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
    match result {
        Ok(()) => match increment_completed(report) {
            Ok(completed) => {
                tracing::info!(
                    target: "sniptale_native_agent::resident_pipe",
                    completed,
                    "resident pipe completed native-host session"
                );
                log_resident_pipe_event(
                    log,
                    "session-completed",
                    "resident pipe completed native-host session",
                    None,
                );
            }
            Err(error) => tracing::warn!(
                target: "sniptale_native_agent::resident_pipe",
                "resident pipe report update failed: {error}"
            ),
        },
        Err(error) => match increment_failed(report) {
            Ok(failed) => {
                tracing::warn!(
                    target: "sniptale_native_agent::resident_pipe",
                    failed,
                    "resident pipe native-host session failed: {error}"
                );
                let message = format!("resident pipe native-host session failed: {error}");
                log_resident_pipe_event(
                    log,
                    "session-failed",
                    &message,
                    Some("resident_pipe.session_failed"),
                );
            }
            Err(report_error) => tracing::warn!(
                target: "sniptale_native_agent::resident_pipe",
                "resident pipe report update failed after session error {error}: {report_error}"
            ),
        },
    }
}
