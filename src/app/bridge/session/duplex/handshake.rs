//! Shared-auth resident-pipe handshake helpers.

use std::{
    io::{Read, Write},
    sync::{
        Arc, Mutex,
        mpsc::{self, RecvTimeoutError},
    },
    thread,
    time::Duration,
};

use crate::{
    app::bridge::session::{
        AgentPipeConnectionConsent, AgentPipeConsentDecision, AgentPipeConsentRequest,
        AgentPipeSessionConfig, AgentPipeSessionError,
    },
    ipc::{
        AuthenticatedPipeSession, PipeHandshakeError, PipeServerAuth,
        read_server_pipe_auth_response, read_server_pipe_hello, write_server_pipe_challenge,
    },
};

use super::{
    logging::{AgentPipeSessionLogSink, clone_log_sink, log_pipe_session_event},
    transport::{AgentPipeConnectionContext, SplitSessionPipe},
};

const MIN_HANDSHAKE_TIMEOUT: Duration = Duration::from_millis(1);

#[derive(Clone, Copy)]
pub(super) struct AgentPipeConsentRuntime<'a> {
    pub(super) consent: Option<&'a dyn AgentPipeConnectionConsent>,
    pub(super) context: Option<&'a AgentPipeConnectionContext>,
}

pub(super) fn authenticate_shared_server_pipe(
    pipe: &mut (impl Read + Write),
    auth: &Arc<Mutex<PipeServerAuth>>,
    config: &AgentPipeSessionConfig,
) -> Result<AuthenticatedPipeSession, AgentPipeSessionError> {
    let hello = read_server_pipe_hello(pipe).map_err(AgentPipeSessionError::Handshake)?;
    let challenge = auth
        .lock()
        .map_err(|_| AgentPipeSessionError::AuthLock)?
        .issue_challenge(
            hello,
            config.now_ms,
            config.challenge_ttl_ms,
            &config.session_id,
            &config.server_nonce,
        )
        .map_err(PipeHandshakeError::Auth)
        .map_err(AgentPipeSessionError::Handshake)?;

    if let Err(error) = write_server_pipe_challenge(pipe, &challenge) {
        discard_pending_challenge(auth, &config.session_id)?;
        return Err(AgentPipeSessionError::Handshake(error));
    }

    let response = match read_server_pipe_auth_response(pipe) {
        Ok(response) => response,
        Err(error) => {
            discard_pending_challenge(auth, &config.session_id)?;
            return Err(AgentPipeSessionError::Handshake(error));
        }
    };

    let authenticate_result = {
        let mut auth = auth.lock().map_err(|_| AgentPipeSessionError::AuthLock)?;
        auth.authenticate(response, config.now_ms.saturating_add(1))
            .map_err(PipeHandshakeError::Auth)
    };
    match authenticate_result {
        Ok(session) => Ok(session),
        Err(error) => {
            discard_pending_challenge(auth, &config.session_id)?;
            Err(AgentPipeSessionError::Handshake(error))
        }
    }
}

pub(super) fn authenticate_shared_server_pipe_with_timeout<R, W, S>(
    reader: &mut R,
    writer: &mut W,
    auth: &Arc<Mutex<PipeServerAuth>>,
    config: &AgentPipeSessionConfig,
    shutdown: &S,
    log: Option<&AgentPipeSessionLogSink>,
    consent_runtime: AgentPipeConsentRuntime<'_>,
) -> Result<AuthenticatedPipeSession, AgentPipeSessionError>
where
    R: Read,
    W: Write,
    S: crate::ipc::AgentPipeReadShutdown + Clone,
{
    let (stop_sender, stop_receiver) = mpsc::sync_channel(1);
    let timeout = handshake_timeout(
        config.challenge_ttl_ms,
        consent_runtime
            .consent
            .map(AgentPipeConnectionConsent::timeout_ms)
            .unwrap_or_default(),
    );
    let shutdown = shutdown.clone();
    let watchdog_log = clone_log_sink(log);
    let session_log = clone_log_sink(log);
    thread::scope(|scope| {
        scope.spawn(move || {
            if stop_receiver.recv_timeout(timeout) == Err(RecvTimeoutError::Timeout) {
                log_pipe_session_event(
                    watchdog_log.as_ref(),
                    "handshake-timeout",
                    "resident pipe handshake timed out",
                    Some("resident_pipe.handshake_timeout"),
                );
                shutdown.shutdown_agent_read();
            }
        });
        let mut pipe = SplitSessionPipe::new(reader, writer);
        let result = authenticate_shared_server_pipe_with_consent(
            &mut pipe,
            auth,
            config,
            consent_runtime,
            session_log.as_ref(),
        );
        let _ = stop_sender.try_send(());
        result
    })
}

fn authenticate_shared_server_pipe_with_consent(
    pipe: &mut (impl Read + Write),
    auth: &Arc<Mutex<PipeServerAuth>>,
    config: &AgentPipeSessionConfig,
    consent_runtime: AgentPipeConsentRuntime<'_>,
    log: Option<&AgentPipeSessionLogSink>,
) -> Result<AuthenticatedPipeSession, AgentPipeSessionError> {
    let session = authenticate_shared_server_pipe(pipe, auth, config)?;
    match confirm_connection(&session, consent_runtime) {
        AgentPipeConsentDecision::Allowed => Ok(session),
        AgentPipeConsentDecision::AllowedWithWarning { code, message } => {
            log_pipe_session_event(log, "connection-consent-warning", message, Some(code));
            Ok(session)
        }
        AgentPipeConsentDecision::Denied { code, message } => {
            auth.lock()
                .map_err(|_| AgentPipeSessionError::AuthLock)?
                .finish_session(&session.session_id);
            log_pipe_session_event(log, "connection-consent-denied", message, Some(code));
            Err(AgentPipeSessionError::ConsentDenied { code, message })
        }
    }
}

fn confirm_connection(
    session: &AuthenticatedPipeSession,
    consent_runtime: AgentPipeConsentRuntime<'_>,
) -> AgentPipeConsentDecision {
    let Some(consent) = consent_runtime.consent else {
        return AgentPipeConsentDecision::Allowed;
    };
    let Some(context) = consent_runtime.context else {
        return AgentPipeConsentDecision::Denied {
            code: "resident_pipe.connection_consent_required",
            message: "resident pipe rejected native-host session: connection consent required",
        };
    };
    consent.confirm_connection(&AgentPipeConsentRequest {
        extension_origin: session.extension_origin.clone(),
        connection: context.clone(),
    })
}

#[cfg(test)]
pub(super) fn authenticate_server_pipe_for_test<R, W>(
    reader: &mut R,
    writer: &mut W,
    auth: &mut PipeServerAuth,
    config: &AgentPipeSessionConfig,
) -> Result<AuthenticatedPipeSession, AgentPipeSessionError>
where
    R: Read,
    W: Write,
{
    let mut pipe = SplitSessionPipe::new(reader, writer);
    crate::ipc::authenticate_server_pipe(
        &mut pipe,
        auth,
        config.now_ms,
        config.challenge_ttl_ms,
        &config.session_id,
        &config.server_nonce,
    )
    .map_err(AgentPipeSessionError::Handshake)
}

#[cfg(test)]
#[path = "handshake_tests.rs"]
mod tests;

fn handshake_timeout(challenge_ttl_ms: u64, consent_timeout_ms: u64) -> Duration {
    Duration::from_millis(challenge_ttl_ms.saturating_add(consent_timeout_ms))
        .max(MIN_HANDSHAKE_TIMEOUT)
}

fn discard_pending_challenge(
    auth: &Arc<Mutex<PipeServerAuth>>,
    session_id: &str,
) -> Result<(), AgentPipeSessionError> {
    auth.lock()
        .map_err(|_| AgentPipeSessionError::AuthLock)?
        .discard_challenge(session_id);
    Ok(())
}
