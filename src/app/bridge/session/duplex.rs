//! Duplex resident agent pipe session with outbound event delivery.

mod authenticated;
mod events;
mod handshake;
mod logging;
mod outbound;
mod progress;
mod reader;
mod transport;
mod writer;

use std::sync::{
    Arc, Mutex,
    mpsc::{Receiver, SyncSender},
};

use crate::{
    app::{
        resident::AgentOutboundEvent,
        responses::{NativeHostSharedState, NativeHostSideEffects},
    },
    ipc::PipeServerAuth,
};

use self::authenticated::run_authenticated_agent_pipe_session;
#[cfg(test)]
use self::handshake::authenticate_server_pipe_for_test;
use self::handshake::{AgentPipeConsentRuntime, authenticate_shared_server_pipe_with_timeout};
use self::logging::AgentPipeSessionLogSink;
use self::progress::{AuthenticatedSessionRuntime, SessionProgress};
pub(crate) use self::transport::{AgentPipeConnectionContext, AgentPipeSplit};
use super::{
    AgentPipeConnectionConsent, AgentPipeSessionConfig, AgentPipeSessionError,
    AgentPipeSessionShutdowns,
};

#[derive(Clone)]
pub(crate) struct AgentPipeSessionRuntime<'a> {
    log: Option<&'a AgentPipeSessionLogSink>,
    shutdowns: Option<&'a AgentPipeSessionShutdowns>,
    ready_sender: Option<SyncSender<()>>,
    consent: Option<&'a dyn AgentPipeConnectionConsent>,
    mode_epoch: Option<crate::app::resident::control::session_epoch::ExtensionSessionEpoch>,
}

impl<'a> AgentPipeSessionRuntime<'a> {
    pub(crate) fn new(
        log: Option<&'a AgentPipeSessionLogSink>,
        shutdowns: Option<&'a AgentPipeSessionShutdowns>,
        ready_sender: Option<SyncSender<()>>,
        consent: Option<&'a dyn AgentPipeConnectionConsent>,
        mode_epoch: Option<crate::app::resident::control::session_epoch::ExtensionSessionEpoch>,
    ) -> Self {
        Self {
            log,
            shutdowns,
            ready_sender,
            consent,
            mode_epoch,
        }
    }

    fn signal_ready(&self) {
        if let Some(sender) = &self.ready_sender {
            let _ = sender.try_send(());
        }
    }
}

#[cfg(test)]
pub(crate) fn run_agent_pipe_session_with_effects_and_events<P>(
    pipe: P,
    auth: &mut PipeServerAuth,
    config: &AgentPipeSessionConfig,
    effects: &mut impl NativeHostSideEffects,
    events: Receiver<AgentOutboundEvent>,
) -> Result<(), AgentPipeSessionError>
where
    P: AgentPipeSplit,
{
    run_agent_pipe_session_with_effects_events_and_state(
        pipe,
        auth,
        config,
        effects,
        events,
        NativeHostSharedState::default(),
    )
}

#[cfg(test)]
pub(crate) fn run_agent_pipe_session_with_effects_events_and_state<P>(
    pipe: P,
    auth: &mut PipeServerAuth,
    config: &AgentPipeSessionConfig,
    effects: &mut impl NativeHostSideEffects,
    events: Receiver<AgentOutboundEvent>,
    shared_state: NativeHostSharedState,
) -> Result<(), AgentPipeSessionError>
where
    P: AgentPipeSplit,
{
    run_agent_pipe_session_with_effects_events_state_and_log(
        pipe,
        auth,
        config,
        effects,
        events,
        shared_state,
        None,
    )
}

#[cfg(test)]
pub(crate) fn run_agent_pipe_session_with_effects_events_state_and_log<P>(
    pipe: P,
    auth: &mut PipeServerAuth,
    config: &AgentPipeSessionConfig,
    effects: &mut impl NativeHostSideEffects,
    events: Receiver<AgentOutboundEvent>,
    shared_state: NativeHostSharedState,
    log: Option<&AgentPipeSessionLogSink>,
) -> Result<(), AgentPipeSessionError>
where
    P: AgentPipeSplit,
{
    let (mut reader, mut writer, shutdown) = pipe
        .split_for_session()
        .map_err(AgentPipeSessionError::Split)?;
    let mut authenticated =
        { authenticate_server_pipe_for_test(&mut reader, &mut writer, auth, config)? };
    let progress = Arc::new(SessionProgress::default());
    let result = run_authenticated_agent_pipe_session(
        reader,
        writer,
        &mut authenticated,
        effects,
        events,
        shared_state,
        AuthenticatedSessionRuntime {
            log,
            progress: &progress,
            shutdown,
            mode_epoch: None,
        },
    );
    auth.finish_session(&authenticated.session_id);
    result
}

pub(crate) fn run_agent_pipe_session_with_effects_events_shared_auth_state_log_and_shutdowns<P>(
    pipe: P,
    auth: &Arc<Mutex<PipeServerAuth>>,
    config: &AgentPipeSessionConfig,
    effects: &mut impl NativeHostSideEffects,
    events: Receiver<AgentOutboundEvent>,
    shared_state: NativeHostSharedState,
    runtime: &AgentPipeSessionRuntime<'_>,
) -> Result<(), AgentPipeSessionError>
where
    P: AgentPipeSplit,
{
    let connection_context = pipe.connection_context().cloned();
    let (mut reader, mut writer, shutdown) = match pipe.split_for_session() {
        Ok(split) => split,
        Err(error) => {
            runtime.signal_ready();
            return Err(AgentPipeSessionError::Split(error));
        }
    };
    let session_shutdown = shutdown.clone();
    let _shutdown_guard = match runtime
        .shutdowns
        .map(|shutdowns| shutdowns.register(&config.session_id, &shutdown))
        .transpose()
    {
        Ok(guard) => {
            runtime.signal_ready();
            guard
        }
        Err(error) => {
            runtime.signal_ready();
            return Err(AgentPipeSessionError::ShutdownRegistry(error));
        }
    };
    let mut authenticated = authenticate_shared_server_pipe_with_timeout(
        &mut reader,
        &mut writer,
        auth,
        config,
        &shutdown,
        runtime.log,
        AgentPipeConsentRuntime {
            consent: runtime.consent,
            context: connection_context.as_ref(),
        },
    )?;
    let progress = Arc::new(SessionProgress::default());
    let result = run_authenticated_agent_pipe_session(
        reader,
        writer,
        &mut authenticated,
        effects,
        events,
        shared_state,
        AuthenticatedSessionRuntime {
            log: runtime.log,
            progress: &progress,
            shutdown: session_shutdown,
            mode_epoch: runtime.mode_epoch.clone(),
        },
    );
    auth.lock()
        .map_err(|_| AgentPipeSessionError::AuthLock)?
        .finish_session(&authenticated.session_id);
    result
}
