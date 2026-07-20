//! Resident pipe session startup readiness.

use std::{
    sync::{Arc, Mutex, mpsc, mpsc::RecvTimeoutError},
    time::Duration,
};

use super::super::{
    AgentPipeServerError, AgentPipeServerReport,
    logging::{AgentPipeServerLogSink, log_resident_pipe_event},
};
use super::report::increment_startup_failures;

const SESSION_START_TIMEOUT: Duration = Duration::from_millis(100);

pub(super) fn wait_for_session_ready(
    ready_receiver: &mpsc::Receiver<()>,
    report: &Arc<Mutex<AgentPipeServerReport>>,
    log: Option<&AgentPipeServerLogSink>,
) -> Result<(), AgentPipeServerError> {
    match ready_receiver.recv_timeout(SESSION_START_TIMEOUT) {
        Ok(()) | Err(RecvTimeoutError::Disconnected) => Ok(()),
        Err(RecvTimeoutError::Timeout) => {
            let startup_failures = increment_startup_failures(report)?;
            tracing::warn!(
                target: "sniptale_native_agent::resident_pipe",
                startup_failures,
                error = %AgentPipeServerError::SessionStartTimeout,
                "resident pipe session start timed out"
            );
            log_resident_pipe_event(
                log,
                "session-start-timeout",
                "resident pipe session start timed out",
                Some("resident_pipe.session_start_timeout"),
            );
            Ok(())
        }
    }
}
