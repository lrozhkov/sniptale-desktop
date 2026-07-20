//! Concurrent duplex resident pipe sessions.

mod readiness;
mod report;
mod runtime;
mod session_result;

use std::{
    sync::mpsc,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use super::logging::{clone_log_sink, log_resident_pipe_event};
use super::{
    AgentPipeAccept, AgentPipeDuplexListener, AgentPipeServerControl, AgentPipeServerError,
    AgentPipeServerReport, AgentPipeSessionConfigSource,
};
use crate::{
    app::{
        bridge::session::{
            AgentPipeSessionRuntime, AgentPipeSessionShutdowns,
            run_agent_pipe_session_with_effects_events_shared_auth_state_log_and_shutdowns,
        },
        resident::{AgentEventHub, control::session_epoch::ExtensionSessionEpoch},
        responses::NativeHostSharedState,
    },
    ipc::PipeServerAuth,
};

use self::readiness::wait_for_session_ready;
use self::report::{increment_accepted, increment_failed, join_finished_sessions, report_snapshot};
pub(crate) use self::runtime::{AgentPipeServerRuntime, AgentPipeSessionEffectsFactory};
use self::session_result::{record_accept_rejection, record_session_result};

const SESSION_BROKER_IDLE_POLL_INTERVAL: Duration = Duration::from_millis(5);

#[cfg(test)]
pub(crate) fn run_agent_pipe_server_with_effects_and_events<L, C, E, F>(
    listener: &mut L,
    auth: &mut PipeServerAuth,
    configs: &mut C,
    effects_factory: &E,
    events: &AgentEventHub,
    control: F,
) -> Result<AgentPipeServerReport, AgentPipeServerError>
where
    L: AgentPipeDuplexListener,
    L::Pipe: Send,
    C: AgentPipeSessionConfigSource,
    E: AgentPipeSessionEffectsFactory,
    F: FnMut(&AgentPipeServerReport) -> AgentPipeServerControl,
{
    run_agent_pipe_server_with_effects_events_and_log(
        listener,
        auth,
        configs,
        effects_factory,
        events,
        &AgentPipeServerRuntime {
            log: None,
            consent: None,
            active_settings: None,
        },
        control,
    )
}

pub(crate) fn run_agent_pipe_server_with_effects_events_and_log<L, C, E, F>(
    listener: &mut L,
    auth: &mut PipeServerAuth,
    configs: &mut C,
    effects_factory: &E,
    events: &AgentEventHub,
    runtime: &AgentPipeServerRuntime<'_>,
    mut control: F,
) -> Result<AgentPipeServerReport, AgentPipeServerError>
where
    L: AgentPipeDuplexListener,
    L::Pipe: Send,
    C: AgentPipeSessionConfigSource,
    E: AgentPipeSessionEffectsFactory,
    F: FnMut(&AgentPipeServerReport) -> AgentPipeServerControl,
{
    let report = Arc::new(Mutex::new(AgentPipeServerReport::default()));
    let shared_auth = Arc::new(Mutex::new(auth.clone()));
    let shared_state = NativeHostSharedState::default();
    let shutdowns = AgentPipeSessionShutdowns::default();
    let max_active_sessions = auth.max_sessions();
    let log = runtime.log;
    let consent = runtime.consent;

    thread::scope(|scope| {
        let mut handles = Vec::new();
        loop {
            join_finished_sessions(&mut handles, &report)?;
            let current_report = report_snapshot(&report)?;
            if control(&current_report) == AgentPipeServerControl::Stop {
                shutdowns
                    .cancel_all()
                    .map_err(|_| AgentPipeServerError::Accept)?;
                break;
            }
            if handles.len() >= max_active_sessions {
                thread::sleep(SESSION_BROKER_IDLE_POLL_INTERVAL);
                continue;
            }
            let pipe = match AgentPipeDuplexListener::accept(listener)? {
                AgentPipeAccept::Connected(pipe) => pipe,
                AgentPipeAccept::Pending => continue,
                AgentPipeAccept::Rejected(rejection) => {
                    record_accept_rejection(&report, log, rejection)?;
                    continue;
                }
                #[cfg(test)]
                AgentPipeAccept::Closed if handles.is_empty() => break,
                #[cfg(test)]
                AgentPipeAccept::Closed => {
                    thread::sleep(SESSION_BROKER_IDLE_POLL_INTERVAL);
                    continue;
                }
            };
            let config = configs
                .next_session_config()
                .map_err(AgentPipeServerError::SessionConfig)?;
            let session_mode_epoch = match admitted_session_mode_epoch(runtime, &report, log)? {
                SessionAdmission::Admitted(gate) => gate,
                SessionAdmission::Rejected => continue,
            };
            let accepted = increment_accepted(&report)?;
            tracing::info!(
                target: "sniptale_native_agent::resident_pipe",
                accepted,
                "resident pipe accepted native-host session"
            );
            log_resident_pipe_event(
                log,
                "session-accepted",
                "resident pipe accepted native-host session",
                None,
            );

            let session_auth = Arc::clone(&shared_auth);
            let session_report = Arc::clone(&report);
            let session_state = shared_state.clone();
            let session_events = events.clone();
            let session_effects_factory = E::clone(effects_factory);
            let session_log = clone_log_sink(log);
            let session_shutdowns = shutdowns.clone();
            let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
            let session_consent = consent;
            handles.push(scope.spawn(move || {
                let mut effects = session_effects_factory.create_effects();
                let events = session_events.subscribe();
                let result =
                    run_agent_pipe_session_with_effects_events_shared_auth_state_log_and_shutdowns(
                        pipe,
                        &session_auth,
                        &config,
                        &mut effects,
                        events,
                        session_state,
                        &AgentPipeSessionRuntime::new(
                            session_log.as_ref(),
                            Some(&session_shutdowns),
                            Some(ready_sender),
                            session_consent,
                            session_mode_epoch,
                        ),
                    );
                record_session_result(&session_report, session_log.as_ref(), result);
            }));
            wait_for_session_ready(&ready_receiver, &report, log)?;
        }
        for handle in handles {
            if handle.join().is_err() {
                increment_failed(&report)?;
            }
        }
        Ok(())
    })?;

    *auth = Arc::try_unwrap(shared_auth)
        .map_err(|_| AgentPipeServerError::Accept)?
        .into_inner()
        .map_err(|_| AgentPipeServerError::Accept)?;
    report_snapshot(&report)
}

enum SessionAdmission {
    Admitted(Option<ExtensionSessionEpoch>),
    Rejected,
}

fn admitted_session_mode_epoch(
    runtime: &AgentPipeServerRuntime<'_>,
    report: &Arc<Mutex<AgentPipeServerReport>>,
    log: Option<&super::logging::AgentPipeServerLogSink>,
) -> Result<SessionAdmission, AgentPipeServerError> {
    let Some(settings) = runtime.active_settings.as_ref() else {
        return Ok(SessionAdmission::Admitted(None));
    };
    if let Ok(gate) = ExtensionSessionEpoch::admit(settings.clone()) {
        return Ok(SessionAdmission::Admitted(Some(gate)));
    }
    increment_failed(report)?;
    log_resident_pipe_event(
        log,
        "session-rejected-mode-epoch",
        "resident pipe rejected stale extension session admission",
        Some("resident_pipe.session_mode_epoch_rejected"),
    );
    Ok(SessionAdmission::Rejected)
}
