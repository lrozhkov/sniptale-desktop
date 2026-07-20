//! Resident agent pipe server accept loop.

mod duplex;
mod logging;

#[cfg(test)]
use std::io::{Read, Write};
use std::{error::Error, fmt};

use crate::app::bridge::session::AgentPipeSessionConfig;
#[cfg(test)]
use crate::app::bridge::session::{
    NoopAgentPipeSideEffects, run_agent_pipe_session_with_effects_and_state,
};
#[cfg(test)]
use crate::app::responses::{NativeHostSharedState, NativeHostSideEffects};
#[cfg(test)]
use crate::ipc::PipeServerAuth;

#[cfg(test)]
pub(crate) use duplex::run_agent_pipe_server_with_effects_and_events;
#[cfg(windows)]
pub(crate) use duplex::run_agent_pipe_server_with_effects_events_and_log;
#[cfg(windows)]
pub(crate) use duplex::{AgentPipeServerRuntime, AgentPipeSessionEffectsFactory};
#[cfg(windows)]
pub(crate) use logging::AgentPipeServerLogSink;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AgentPipeServerControl {
    Continue,
    Stop,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct AgentPipeServerReport {
    pub(crate) accepted: usize,
    pub(crate) completed: usize,
    pub(crate) failed: usize,
    pub(crate) startup_failures: usize,
}

#[derive(Debug)]
pub(crate) enum AgentPipeServerError {
    Accept,
    SessionStartTimeout,
    SessionConfig(AgentPipeSessionConfigError),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AgentPipeSessionConfigError {
    Entropy,
}

impl fmt::Display for AgentPipeServerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Accept => formatter.write_str("pipe server accept failed"),
            Self::SessionStartTimeout => formatter.write_str("pipe session start timed out"),
            Self::SessionConfig(error) => write!(formatter, "pipe session config failed: {error}"),
        }
    }
}

impl Error for AgentPipeServerError {}

impl fmt::Display for AgentPipeSessionConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Entropy => formatter.write_str("pipe session entropy failed"),
        }
    }
}

impl Error for AgentPipeSessionConfigError {}

pub(crate) trait AgentPipeDuplexListener {
    type Pipe: crate::app::bridge::session::AgentPipeSplit;

    fn accept(&mut self) -> Result<AgentPipeAccept<Self::Pipe>, AgentPipeServerError>;
}

#[cfg(test)]
pub(crate) trait AgentPipeListener {
    type Pipe: Read + Write;

    fn accept(&mut self) -> Result<AgentPipeAccept<Self::Pipe>, AgentPipeServerError>;
}

#[derive(Debug)]
pub(crate) enum AgentPipeAccept<Pipe> {
    Connected(Pipe),
    Pending,
    Rejected(AgentPipeRejection),
    #[cfg(test)]
    Closed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct AgentPipeRejection {
    pub(crate) phase: &'static str,
    pub(crate) code: &'static str,
    pub(crate) message: &'static str,
}

impl AgentPipeRejection {
    pub(crate) const fn new(
        phase: &'static str,
        code: &'static str,
        message: &'static str,
    ) -> Self {
        Self {
            phase,
            code,
            message,
        }
    }
}

pub(crate) trait AgentPipeSessionConfigSource {
    fn next_session_config(
        &mut self,
    ) -> Result<AgentPipeSessionConfig, AgentPipeSessionConfigError>;
}

pub(crate) trait AgentPipeEntropy {
    fn next_bytes(&mut self) -> Result<[u8; 16], AgentPipeSessionConfigError>;
}

pub(crate) trait AgentPipeClock {
    fn now_ms(&mut self) -> u64;
}

#[derive(Clone, Debug)]
pub(crate) struct AgentPipeSessionConfigGenerator<E, C> {
    entropy: E,
    clock: C,
    challenge_ttl_ms: u64,
}

impl<E, C> AgentPipeSessionConfigGenerator<E, C> {
    pub(crate) const fn new(entropy: E, clock: C, challenge_ttl_ms: u64) -> Self {
        Self {
            entropy,
            clock,
            challenge_ttl_ms,
        }
    }
}

impl<E, C> AgentPipeSessionConfigSource for AgentPipeSessionConfigGenerator<E, C>
where
    E: AgentPipeEntropy,
    C: AgentPipeClock,
{
    fn next_session_config(
        &mut self,
    ) -> Result<AgentPipeSessionConfig, AgentPipeSessionConfigError> {
        let session_id = format!("session-{}", hex16(self.entropy.next_bytes()?));
        let server_nonce = format!("server-nonce-{}", hex16(self.entropy.next_bytes()?));
        Ok(AgentPipeSessionConfig {
            now_ms: self.clock.now_ms(),
            challenge_ttl_ms: self.challenge_ttl_ms,
            session_id,
            server_nonce,
        })
    }
}

#[cfg(test)]
pub(crate) fn run_agent_pipe_server<L, C, F>(
    listener: &mut L,
    auth: &mut PipeServerAuth,
    configs: &mut C,
    control: F,
) -> Result<AgentPipeServerReport, AgentPipeServerError>
where
    L: AgentPipeListener,
    C: AgentPipeSessionConfigSource,
    F: FnMut(&AgentPipeServerReport) -> AgentPipeServerControl,
{
    let mut effects = NoopAgentPipeSideEffects;
    run_agent_pipe_server_with_effects(listener, auth, configs, &mut effects, control)
}

#[cfg(test)]
pub(crate) fn run_agent_pipe_server_with_effects<L, C, E, F>(
    listener: &mut L,
    auth: &mut PipeServerAuth,
    configs: &mut C,
    effects: &mut E,
    mut control: F,
) -> Result<AgentPipeServerReport, AgentPipeServerError>
where
    L: AgentPipeListener,
    C: AgentPipeSessionConfigSource,
    E: NativeHostSideEffects,
    F: FnMut(&AgentPipeServerReport) -> AgentPipeServerControl,
{
    let mut report = AgentPipeServerReport::default();
    let shared_state = NativeHostSharedState::default();
    loop {
        if control(&report) == AgentPipeServerControl::Stop {
            break;
        }
        let mut pipe = match listener.accept()? {
            AgentPipeAccept::Connected(pipe) => pipe,
            AgentPipeAccept::Pending => continue,
            AgentPipeAccept::Rejected(_) => {
                report.failed += 1;
                continue;
            }
            #[cfg(test)]
            AgentPipeAccept::Closed => break,
        };
        report.accepted += 1;
        let config = configs
            .next_session_config()
            .map_err(AgentPipeServerError::SessionConfig)?;
        match run_agent_pipe_session_with_effects_and_state(
            &mut pipe,
            auth,
            &config,
            effects,
            shared_state.clone(),
        ) {
            Ok(()) => report.completed += 1,
            Err(_) => report.failed += 1,
        }
    }
    Ok(report)
}

fn hex16(bytes: [u8; 16]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(32);
    for byte in bytes {
        output.push(DIGITS[(byte >> 4) as usize] as char);
        output.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    output
}
