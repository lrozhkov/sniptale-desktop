//! Resident agent pipe session coordinator.

mod duplex;
mod session_shutdown;

#[cfg(test)]
use std::io::Read;
use std::{error::Error, fmt, io::Write};

use serde_json::Value;

#[cfg(test)]
use crate::app::responses::{
    NativeHostSession, NativeHostSharedState, NativeHostSideEffectError, NativeHostSideEffects,
    responses_for_message_with_effects,
};
#[cfg(test)]
use crate::hotkeys::HotkeyRegistrationPlan;
use crate::ipc::write_payload;
use crate::{
    ipc::{
        AGENT_PIPE_RESPONSE_HEADER_MAX_BYTES, NativeMessagingError, PipeAuthError,
        PipeHandshakeError, agent_pipe_response_header_payload,
    },
    protocol::{HOST_TO_EXTENSION_MAX_MESSAGE_BYTES, ParseError, parse_wire_message_with_limit},
};
#[cfg(test)]
use crate::{
    ipc::{PipeServerAuth, authenticate_server_pipe, read_payload},
    protocol::{EXTENSION_TO_HOST_MAX_MESSAGE_BYTES, parse_wire_message},
};

#[cfg(test)]
pub(crate) use duplex::run_agent_pipe_session_with_effects_and_events;
#[cfg(test)]
pub(crate) use duplex::run_agent_pipe_session_with_effects_events_and_state;
pub(crate) use duplex::{
    AgentPipeConnectionContext, AgentPipeSessionRuntime, AgentPipeSplit,
    run_agent_pipe_session_with_effects_events_shared_auth_state_log_and_shutdowns,
};
pub(crate) use session_shutdown::{AgentPipeSessionShutdownError, AgentPipeSessionShutdowns};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AgentPipeSessionConfig {
    pub(crate) now_ms: u64,
    pub(crate) challenge_ttl_ms: u64,
    pub(crate) session_id: String,
    pub(crate) server_nonce: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AgentPipeConsentRequest {
    pub(crate) extension_origin: String,
    pub(crate) connection: AgentPipeConnectionContext,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AgentPipeConsentDecision {
    Allowed,
    AllowedWithWarning {
        code: &'static str,
        message: &'static str,
    },
    Denied {
        code: &'static str,
        message: &'static str,
    },
}

pub(crate) trait AgentPipeConnectionConsent: Sync {
    fn confirm_connection(&self, request: &AgentPipeConsentRequest) -> AgentPipeConsentDecision;

    fn timeout_ms(&self) -> u64 {
        30_000
    }
}

#[derive(Debug)]
pub(crate) enum AgentPipeSessionError {
    AuthLock,
    Handshake(PipeHandshakeError),
    Frame(NativeMessagingError),
    Protocol(ParseError),
    Json(serde_json::Error),
    ConsentDenied {
        code: &'static str,
        message: &'static str,
    },
    Backpressure(PipeAuthError),
    ResponseHeader(crate::ipc::AgentPipeResponseHeaderError),
    ResponseQueueClosed,
    ModeEpochChanged,
    ShutdownRegistry(AgentPipeSessionShutdownError),
    Split(std::io::Error),
}

impl fmt::Display for AgentPipeSessionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AuthLock => formatter.write_str("agent pipe auth lock failed"),
            Self::Handshake(error) => write!(formatter, "agent pipe handshake failed: {error}"),
            Self::Frame(error) => write!(formatter, "agent pipe frame failed: {error}"),
            Self::Protocol(error) => write!(formatter, "agent pipe protocol rejected: {error}"),
            Self::Json(error) => write!(formatter, "agent pipe JSON failed: {error}"),
            Self::ConsentDenied { code, message } => write!(formatter, "{message}: {code}"),
            Self::Backpressure(error) => write!(formatter, "agent pipe backpressure: {error}"),
            Self::ResponseHeader(error) => {
                write!(formatter, "agent pipe response header failed: {error}")
            }
            Self::ResponseQueueClosed => formatter.write_str("agent pipe response queue closed"),
            Self::ModeEpochChanged => formatter.write_str("agent pipe mode epoch changed"),
            Self::ShutdownRegistry(_) => formatter.write_str("agent pipe shutdown registry failed"),
            Self::Split(error) => write!(formatter, "agent pipe split failed: {error}"),
        }
    }
}

impl Error for AgentPipeSessionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Handshake(error) => Some(error),
            Self::Frame(error) => Some(error),
            Self::Protocol(error) => Some(error),
            Self::Json(error) => Some(error),
            Self::Backpressure(error) => Some(error),
            Self::AuthLock
            | Self::ConsentDenied { .. }
            | Self::ResponseQueueClosed
            | Self::ModeEpochChanged
            | Self::ShutdownRegistry(_) => None,
            Self::ResponseHeader(error) => Some(error),
            Self::Split(error) => Some(error),
        }
    }
}

#[cfg(test)]
pub(crate) fn run_agent_pipe_session(
    pipe: &mut (impl Read + Write),
    auth: &mut PipeServerAuth,
    config: &AgentPipeSessionConfig,
) -> Result<(), AgentPipeSessionError> {
    let mut effects = NoopAgentPipeSideEffects;
    run_agent_pipe_session_with_effects(pipe, auth, config, &mut effects)
}

#[cfg(test)]
pub(crate) fn run_agent_pipe_session_with_effects(
    pipe: &mut (impl Read + Write),
    auth: &mut PipeServerAuth,
    config: &AgentPipeSessionConfig,
    effects: &mut impl NativeHostSideEffects,
) -> Result<(), AgentPipeSessionError> {
    run_agent_pipe_session_with_effects_and_state(
        pipe,
        auth,
        config,
        effects,
        NativeHostSharedState::default(),
    )
}

#[cfg(test)]
pub(crate) fn run_agent_pipe_session_with_effects_and_state(
    pipe: &mut (impl Read + Write),
    auth: &mut PipeServerAuth,
    config: &AgentPipeSessionConfig,
    effects: &mut impl NativeHostSideEffects,
    shared_state: NativeHostSharedState,
) -> Result<(), AgentPipeSessionError> {
    let mut session = authenticate_server_pipe(
        pipe,
        auth,
        config.now_ms,
        config.challenge_ttl_ms,
        &config.session_id,
        &config.server_nonce,
    )
    .map_err(AgentPipeSessionError::Handshake)?;
    #[cfg(test)]
    let mut native_host_session = NativeHostSession::with_shared_state(shared_state);
    #[cfg(not(test))]
    let mut native_host_session = NativeHostSession::system_clocked_with_shared_state(shared_state);

    let result = (|| {
        while let Some(payload) = read_payload(pipe, EXTENSION_TO_HOST_MAX_MESSAGE_BYTES)
            .map_err(AgentPipeSessionError::Frame)?
        {
            session
                .enqueue_forwarded_message()
                .map_err(AgentPipeSessionError::Backpressure)?;
            handle_forwarded_payload(pipe, &mut native_host_session, &payload, effects)?;
            session.complete_forwarded_message();
        }
        Ok(())
    })();
    if let Some(generation) = native_host_session.release_controller_lease() {
        let _ = effects.mark_tray_connection_disconnected(generation);
    }
    auth.finish_session(&session.session_id);
    result
}

#[cfg(test)]
fn handle_forwarded_payload(
    pipe: &mut impl Write,
    session: &mut NativeHostSession,
    payload: &[u8],
    effects: &mut impl NativeHostSideEffects,
) -> Result<(), AgentPipeSessionError> {
    let message = parse_wire_message(payload).map_err(AgentPipeSessionError::Protocol)?;
    let value: Value = serde_json::from_slice(payload).map_err(AgentPipeSessionError::Json)?;
    let responses = responses_for_message_with_effects(session, &message, &value, effects);
    write_response_batch(pipe, responses)
}

pub(super) fn write_response_batch(
    pipe: &mut impl Write,
    responses: Vec<Value>,
) -> Result<(), AgentPipeSessionError> {
    let header_payload = agent_pipe_response_header_payload(responses.len())
        .map_err(AgentPipeSessionError::ResponseHeader)?;
    write_payload(pipe, &header_payload, AGENT_PIPE_RESPONSE_HEADER_MAX_BYTES)
        .map_err(AgentPipeSessionError::Frame)?;
    for response in responses {
        let response_payload =
            serde_json::to_vec(&response).map_err(AgentPipeSessionError::Json)?;
        parse_wire_message_with_limit(&response_payload, HOST_TO_EXTENSION_MAX_MESSAGE_BYTES)
            .map_err(AgentPipeSessionError::Protocol)?;
        write_payload(pipe, &response_payload, HOST_TO_EXTENSION_MAX_MESSAGE_BYTES)
            .map_err(AgentPipeSessionError::Frame)?;
    }
    Ok(())
}

#[cfg(test)]
pub(crate) struct NoopAgentPipeSideEffects;

#[cfg(test)]
impl NativeHostSideEffects for NoopAgentPipeSideEffects {
    fn apply_hotkey_plan(
        &mut self,
        _plan: HotkeyRegistrationPlan,
    ) -> Result<(), NativeHostSideEffectError> {
        Ok(())
    }
}
