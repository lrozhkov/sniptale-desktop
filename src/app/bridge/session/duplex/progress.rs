//! Authenticated resident-pipe session progress deadlines.

use std::{
    sync::{
        atomic::{AtomicBool, AtomicU8, Ordering},
        mpsc::{Receiver, RecvTimeoutError},
    },
    time::Duration,
};

use serde_json::Value;

use super::logging::AgentPipeSessionLogSink;
use crate::protocol::WireMessage;

const CONTROLLER_ACQUIRE_TIMEOUT: Duration = Duration::from_secs(5);
const SETTINGS_ACCEPT_TIMEOUT: Duration = Duration::from_secs(10);
const SESSION_PROGRESS_STARTED: u8 = 0;
const SESSION_PROGRESS_CONTROLLER: u8 = 1;
const SESSION_PROGRESS_SETTINGS: u8 = 2;

#[derive(Default)]
pub(crate) struct SessionProgress {
    state: AtomicU8,
}

pub(super) struct AuthenticatedSessionRuntime<'a, S> {
    pub(super) log: Option<&'a AgentPipeSessionLogSink>,
    pub(super) progress: &'a std::sync::Arc<SessionProgress>,
    pub(super) shutdown: S,
    pub(super) mode_epoch:
        Option<crate::app::resident::control::session_epoch::ExtensionSessionEpoch>,
}

impl SessionProgress {
    pub(crate) fn observe_result(&self, message: &WireMessage, responses: &[Value]) {
        match message {
            WireMessage::ExtensionControllerAcquire(_) => {
                self.mark_at_least(SESSION_PROGRESS_CONTROLLER);
            }
            WireMessage::ExtensionSettingsSync(_) if includes_settings_accepted(responses) => {
                self.mark_at_least(SESSION_PROGRESS_SETTINGS);
            }
            _ => {}
        }
    }

    fn mark_at_least(&self, target: u8) {
        let mut current = self.state.load(Ordering::Acquire);
        while current < target {
            match self
                .state
                .compare_exchange(current, target, Ordering::AcqRel, Ordering::Acquire)
            {
                Ok(_) => return,
                Err(next) => current = next,
            }
        }
    }

    fn current(&self) -> u8 {
        self.state.load(Ordering::Acquire)
    }
}

fn includes_settings_accepted(responses: &[Value]) -> bool {
    responses.iter().any(|response| {
        response
            .get("type")
            .and_then(Value::as_str)
            .is_some_and(|message_type| message_type == "app.settings.accepted")
    })
}

pub(super) fn run_session_progress_watchdog(
    progress: &SessionProgress,
    running: &AtomicBool,
    shutdown: &impl crate::ipc::AgentPipeReadShutdown,
    stop: &Receiver<()>,
    log: Option<&AgentPipeSessionLogSink>,
) {
    if stop.recv_timeout(CONTROLLER_ACQUIRE_TIMEOUT) != Err(RecvTimeoutError::Timeout) {
        return;
    }
    if !running.load(Ordering::Acquire) {
        return;
    }
    if progress.current() == SESSION_PROGRESS_STARTED {
        super::logging::log_pipe_session_event(
            log,
            "session-timeout-no-controller",
            "resident pipe session timed out before controller acquisition",
            Some("resident_pipe.session_timeout_no_controller"),
        );
        shutdown.shutdown_agent_read();
        return;
    }
    if stop.recv_timeout(SETTINGS_ACCEPT_TIMEOUT.saturating_sub(CONTROLLER_ACQUIRE_TIMEOUT))
        != Err(RecvTimeoutError::Timeout)
    {
        return;
    }
    if !running.load(Ordering::Acquire) {
        return;
    }
    if progress.current() < SESSION_PROGRESS_SETTINGS {
        super::logging::log_pipe_session_event(
            log,
            "session-timeout-no-settings",
            "resident pipe session timed out before settings acceptance",
            Some("resident_pipe.session_timeout_no_settings"),
        );
        shutdown.shutdown_agent_read();
    }
}
