//! Resident pipe outbound event forwarder.

use std::{
    io::Write,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{Receiver, RecvTimeoutError},
    },
    time::Duration,
};

use crate::app::{
    resident::{AgentOutboundEvent, control::session_epoch::ExtensionSessionEpoch},
    responses::NativeHostSession,
};

use super::{
    events::responses_for_agent_event,
    logging::{AgentPipeSessionLogSink, log_pipe_session_event, response_type_summary},
    writer::write_response_batch_with_log,
};
use crate::app::bridge::session::AgentPipeSessionError;

const OUTBOUND_POLL_INTERVAL: Duration = Duration::from_millis(25);

pub(super) fn run_outbound_event_forwarder<W>(
    events: &Receiver<AgentOutboundEvent>,
    session: &Arc<Mutex<NativeHostSession>>,
    writer: &Arc<Mutex<W>>,
    running: &Arc<AtomicBool>,
    error: &Arc<Mutex<Option<AgentPipeSessionError>>>,
    log: Option<&AgentPipeSessionLogSink>,
    mode_epoch: Option<&ExtensionSessionEpoch>,
) where
    W: Write + Send,
{
    while running.load(Ordering::Acquire) {
        match events.recv_timeout(OUTBOUND_POLL_INTERVAL) {
            Ok(event) => {
                log_pipe_session_event(
                    log,
                    "outbound-event-received",
                    "resident pipe outbound event received",
                    None,
                );
                if !write_outbound_event(event, session, writer, error, log, mode_epoch) {
                    break;
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}

fn write_outbound_event<W>(
    event: AgentOutboundEvent,
    session: &Arc<Mutex<NativeHostSession>>,
    writer: &Arc<Mutex<W>>,
    error: &Arc<Mutex<Option<AgentPipeSessionError>>>,
    log: Option<&AgentPipeSessionLogSink>,
    mode_epoch: Option<&ExtensionSessionEpoch>,
) -> bool
where
    W: Write + Send,
{
    if mode_epoch.is_some_and(|gate| !gate.remains_current()) {
        if let Ok(mut slot) = error.lock() {
            *slot = Some(AgentPipeSessionError::ModeEpochChanged);
        }
        return false;
    }
    let responses = match session.lock() {
        Ok(mut session) => responses_for_agent_event(&mut session, event),
        Err(_) => Vec::new(),
    };
    if responses.is_empty() {
        return true;
    }
    let response_count = responses.len();
    let response_types = response_type_summary(&responses);
    let message = format!(
        "resident pipe starting outbound response write responseCount={response_count} responseTypes={response_types}"
    );
    log_pipe_session_event(log, "outbound-write-start", &message, None);
    let write_result = match writer.lock() {
        Ok(mut writer) => write_response_batch_with_log(&mut *writer, responses, log),
        Err(_) => Err(AgentPipeSessionError::ResponseQueueClosed),
    };
    if let Err(write_error) = write_result {
        if let Ok(mut slot) = error.lock() {
            *slot = Some(write_error);
        }
        return false;
    }
    let message = format!(
        "resident pipe wrote outbound response batch responseCount={response_count} responseTypes={response_types}"
    );
    log_pipe_session_event(log, "outbound-response-written", &message, None);
    true
}
