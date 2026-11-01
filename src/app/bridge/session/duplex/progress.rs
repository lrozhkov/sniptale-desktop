//! Authenticated resident-pipe session progress deadlines.

use std::{
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicU8, Ordering},
        mpsc::{Receiver, RecvTimeoutError},
    },
    time::{Duration, Instant},
};

use serde_json::Value;

use super::logging::AgentPipeSessionLogSink;
use crate::protocol::WireMessage;

const HELLO_TIMEOUT: Duration = Duration::from_secs(15);
const HUMAN_TRUST_TIMEOUT: Duration = Duration::from_secs(300);
const HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(45);

const SESSION_PROGRESS_STARTED: u8 = 0;
const SESSION_PROGRESS_HELLO: u8 = 1;
const SESSION_PROGRESS_CONTROLLER: u8 = 2;

pub(crate) struct SessionProgress {
    state: AtomicU8,
    started: Instant,
    last_activity: Mutex<Instant>,
}

impl Default for SessionProgress {
    fn default() -> Self {
        let now = Instant::now();
        Self {
            state: AtomicU8::new(0),
            started: now,
            last_activity: Mutex::new(now),
        }
    }
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
        if let Ok(mut activity) = self.last_activity.lock() {
            *activity = Instant::now();
        }
        match message {
            WireMessage::ExtensionHello(_)
                if responses.iter().any(|value| value["type"] == "app.hello") =>
            {
                self.mark_at_least(SESSION_PROGRESS_HELLO);
            }
            WireMessage::ExtensionControllerAcquire(_)
                if responses
                    .iter()
                    .any(|value| value["type"] == "app.controller.lease") =>
            {
                self.mark_at_least(SESSION_PROGRESS_CONTROLLER);
            }
            _ => {}
        }
    }

    fn timeout_category(&self, now: Instant) -> Option<&'static str> {
        match self.current() {
            SESSION_PROGRESS_STARTED
                if now.saturating_duration_since(self.started) > HELLO_TIMEOUT =>
            {
                Some("hello-timeout")
            }
            SESSION_PROGRESS_HELLO
                if now.saturating_duration_since(self.started) > HUMAN_TRUST_TIMEOUT =>
            {
                Some("trust-timeout")
            }
            SESSION_PROGRESS_CONTROLLER
                if self.last_activity.lock().map_or(true, |activity| {
                    now.saturating_duration_since(*activity) > HEARTBEAT_TIMEOUT
                }) =>
            {
                Some("heartbeat-timeout")
            }
            _ => None,
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

pub(super) fn run_session_progress_watchdog(
    progress: &SessionProgress,
    running: &AtomicBool,
    shutdown: &impl crate::ipc::AgentPipeReadShutdown,
    stop: &Receiver<()>,
    log: Option<&AgentPipeSessionLogSink>,
) {
    while running.load(Ordering::Acquire) {
        if stop.recv_timeout(Duration::from_millis(100)) != Err(RecvTimeoutError::Timeout) {
            return;
        }
        if let Some(category) = progress.timeout_category(Instant::now()) {
            super::logging::log_pipe_session_event(
                log,
                category,
                "resident pipe session progress deadline expired",
                Some("resident_pipe.session_progress_timeout"),
            );
            shutdown.shutdown_agent_read();
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_trust_wait_and_observer_heartbeat_have_separate_bounded_deadlines() {
        let progress = SessionProgress::default();
        assert_eq!(
            progress.timeout_category(progress.started + Duration::from_secs(16)),
            Some("hello-timeout")
        );
        progress.mark_at_least(SESSION_PROGRESS_HELLO);
        assert_eq!(
            progress.timeout_category(progress.started + Duration::from_secs(60)),
            None
        );
        assert_eq!(
            progress.timeout_category(progress.started + Duration::from_secs(301)),
            Some("trust-timeout")
        );
        progress.mark_at_least(SESSION_PROGRESS_CONTROLLER);
        assert_eq!(
            progress.timeout_category(progress.started + Duration::from_secs(46)),
            Some("heartbeat-timeout")
        );
    }
}
