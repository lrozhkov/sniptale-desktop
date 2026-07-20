//! Resident pipe forwarded-message reader.

use std::{
    io::{Read, Write},
    sync::{Arc, Mutex},
};

use serde_json::Value;

use super::SessionProgress;
use super::logging::{AgentPipeSessionLogSink, log_pipe_session_event, response_type_summary};
use super::writer::write_response_batch_with_log;
use crate::{
    app::{
        bridge::session::AgentPipeSessionError,
        resident::control::session_epoch::ExtensionSessionEpoch,
        responses::{NativeHostSession, NativeHostSideEffects, responses_for_message_with_effects},
    },
    ipc::{AuthenticatedPipeSession, read_payload},
    protocol::{EXTENSION_TO_HOST_MAX_MESSAGE_BYTES, parse_wire_message},
};

pub(super) struct ForwardedPayloadRuntime<'a> {
    pub(super) log: Option<&'a AgentPipeSessionLogSink>,
    pub(super) progress: &'a SessionProgress,
    pub(super) mode_epoch: Option<&'a ExtensionSessionEpoch>,
}

pub(super) fn run_forwarded_payload_reader(
    reader: &mut impl Read,
    writer: &Arc<Mutex<impl Write>>,
    session: &Arc<Mutex<NativeHostSession>>,
    authenticated: &mut AuthenticatedPipeSession,
    effects: &mut impl NativeHostSideEffects,
    runtime: &ForwardedPayloadRuntime<'_>,
) -> Result<(), AgentPipeSessionError> {
    while let Some(responses) =
        read_forwarded_response_batch(reader, session, authenticated, effects, runtime)?
    {
        let mut writer = writer
            .lock()
            .map_err(|_| AgentPipeSessionError::ResponseQueueClosed)?;
        write_response_batch_with_log(&mut *writer, responses, runtime.log)?;
    }
    log_pipe_session_event(
        runtime.log,
        "session-clean-close",
        "resident pipe reader observed closed transport",
        None,
    );
    Ok(())
}

pub(super) fn read_forwarded_response_batch(
    reader: &mut impl Read,
    session: &Arc<Mutex<NativeHostSession>>,
    authenticated: &mut AuthenticatedPipeSession,
    effects: &mut impl NativeHostSideEffects,
    runtime: &ForwardedPayloadRuntime<'_>,
) -> Result<Option<Vec<Value>>, AgentPipeSessionError> {
    if runtime
        .mode_epoch
        .is_some_and(|gate| !gate.remains_current())
    {
        return Err(AgentPipeSessionError::ModeEpochChanged);
    }
    authenticated
        .enqueue_forwarded_message()
        .map_err(AgentPipeSessionError::Backpressure)?;
    let Some(payload) = read_forwarded_payload_with_permit(reader, authenticated)? else {
        return Ok(None);
    };
    let result = match session.lock() {
        Ok(mut session) => {
            responses_for_forwarded_payload(&mut session, &payload, effects, runtime.progress)
        }
        Err(_) => Err(AgentPipeSessionError::ResponseQueueClosed),
    };
    authenticated.complete_forwarded_message();
    let (message_type, responses) = result?;
    let response_count = responses.len();
    let response_types = response_type_summary(&responses);
    let message = format!("resident pipe received message type={message_type}");
    log_pipe_session_event(runtime.log, "message-received", &message, None);
    let message = format!(
        "resident pipe prepared response batch responseCount={response_count} responseTypes={response_types}"
    );
    log_pipe_session_event(runtime.log, "response-batch-prepared", &message, None);
    Ok(Some(responses))
}

fn read_forwarded_payload_with_permit(
    reader: &mut impl Read,
    authenticated: &mut AuthenticatedPipeSession,
) -> Result<Option<Vec<u8>>, AgentPipeSessionError> {
    match read_forwarded_payload(reader) {
        Ok(Some(payload)) => Ok(Some(payload)),
        Ok(None) => {
            authenticated.complete_forwarded_message();
            Ok(None)
        }
        Err(error) => {
            authenticated.complete_forwarded_message();
            Err(error)
        }
    }
}

fn read_forwarded_payload(
    reader: &mut impl Read,
) -> Result<Option<Vec<u8>>, AgentPipeSessionError> {
    match read_payload(reader, EXTENSION_TO_HOST_MAX_MESSAGE_BYTES) {
        Ok(payload) => Ok(payload),
        Err(error) if error.is_closed_transport() => Ok(None),
        Err(error) => Err(AgentPipeSessionError::Frame(error)),
    }
}

fn responses_for_forwarded_payload(
    session: &mut NativeHostSession,
    payload: &[u8],
    effects: &mut impl NativeHostSideEffects,
    progress: &SessionProgress,
) -> Result<(String, Vec<Value>), AgentPipeSessionError> {
    let message = parse_wire_message(payload).map_err(AgentPipeSessionError::Protocol)?;
    let value: Value = serde_json::from_slice(payload).map_err(AgentPipeSessionError::Json)?;
    let message_type = value
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_owned();
    let responses = responses_for_message_with_effects(session, &message, &value, effects);
    progress.observe_result(&message, &responses);
    Ok((message_type, responses))
}
