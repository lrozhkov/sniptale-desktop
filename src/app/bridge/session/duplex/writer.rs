//! Duplex pipe response writer.

use std::io::Write;

use serde_json::Value;

use super::logging::{AgentPipeSessionLogSink, log_pipe_session_event, response_type_summary};
use crate::app::bridge::session::{AgentPipeSessionError, write_response_batch};

pub(super) fn write_response_batch_with_log(
    writer: &mut impl Write,
    responses: Vec<Value>,
    log: Option<&AgentPipeSessionLogSink>,
) -> Result<(), AgentPipeSessionError> {
    let response_count = responses.len();
    let response_types = response_type_summary(&responses);
    if let Err(write_error) = write_response_batch(writer, responses) {
        if is_closed_transport(&write_error) {
            log_pipe_session_event(
                log,
                "session-clean-close",
                "resident pipe writer observed closed transport",
                None,
            );
            return Ok(());
        }
        return Err(write_error);
    }
    let message = format!(
        "resident pipe wrote response batch responseCount={response_count} responseTypes={response_types}"
    );
    log_pipe_session_event(log, "response-batch-written", &message, None);
    Ok(())
}

fn is_closed_transport(error: &AgentPipeSessionError) -> bool {
    matches!(error, AgentPipeSessionError::Frame(frame) if frame.is_closed_transport())
}
