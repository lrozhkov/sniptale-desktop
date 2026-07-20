//! Authenticated native-host bridge wrappers.

#[cfg(any(windows, test))]
use std::io;
#[cfg(any(windows, test))]
use std::io::{Read, Write};
use std::{error::Error, fmt};

#[cfg(any(windows, test))]
use super::AgentPipeReadShutdown;
use super::BridgeForwardError;
#[cfg(any(windows, test))]
use super::BridgeForwardLogSink;
#[cfg(test)]
use super::forward_native_host_stream;
#[cfg(any(windows, test))]
use super::forward_native_host_stream_duplex_with_shutdown;
#[cfg(test)]
use super::forward_native_host_stream_with_log;
use crate::ipc::PipeHandshakeError;
#[cfg(any(windows, test))]
use crate::ipc::{BridgeHello, PipeSecret, authenticate_client_pipe};

#[derive(Debug)]
pub(crate) enum AuthenticatedBridgeForwardError {
    Handshake(PipeHandshakeError),
    #[cfg(any(windows, test))]
    Split(io::Error),
    Forward(BridgeForwardError),
}

impl fmt::Display for AuthenticatedBridgeForwardError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Handshake(error) => write!(formatter, "pipe handshake failed: {error}"),
            #[cfg(any(windows, test))]
            Self::Split(error) => write!(formatter, "pipe split failed: {error}"),
            Self::Forward(error) => write!(formatter, "pipe forwarding failed: {error}"),
        }
    }
}

impl Error for AuthenticatedBridgeForwardError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Handshake(error) => Some(error),
            #[cfg(any(windows, test))]
            Self::Split(error) => Some(error),
            Self::Forward(error) => Some(error),
        }
    }
}

#[cfg(any(windows, test))]
pub(crate) trait AuthenticatedDuplexAgentPipe: Read + Write + Sized {
    type Reader: Read + Send;
    type Writer: Write + Send;
    type Shutdown: AgentPipeReadShutdown;

    fn split_for_duplex(self) -> io::Result<(Self::Reader, Self::Writer, Self::Shutdown)>;
}

#[cfg(windows)]
impl AuthenticatedDuplexAgentPipe for crate::ipc::WindowsPipe {
    type Reader = crate::ipc::WindowsPipeReader;
    type Writer = crate::ipc::WindowsPipeWriter;
    type Shutdown = crate::ipc::WindowsPipeShutdown;

    fn split_for_duplex(self) -> io::Result<(Self::Reader, Self::Writer, Self::Shutdown)> {
        self.split_with_shutdown()
    }
}

#[cfg(windows)]
impl AgentPipeReadShutdown for crate::ipc::WindowsPipeShutdown {
    fn shutdown_agent_read(&self) {
        self.cancel_pending_read();
    }
}

#[cfg(test)]
pub(crate) fn forward_authenticated_native_host_stream(
    browser_reader: &mut impl Read,
    browser_writer: &mut impl Write,
    agent_pipe: &mut (impl Read + Write),
    hello: &BridgeHello,
    secret: &PipeSecret,
) -> Result<(), AuthenticatedBridgeForwardError> {
    forward_authenticated_native_host_stream_with_authenticated_hook(
        browser_reader,
        browser_writer,
        agent_pipe,
        hello,
        secret,
        || {},
    )
}

#[cfg(test)]
pub(crate) fn forward_authenticated_native_host_stream_with_authenticated_hook(
    browser_reader: &mut impl Read,
    browser_writer: &mut impl Write,
    agent_pipe: &mut (impl Read + Write),
    hello: &BridgeHello,
    secret: &PipeSecret,
    mut on_authenticated: impl FnMut(),
) -> Result<(), AuthenticatedBridgeForwardError> {
    authenticate_client_pipe(agent_pipe, hello, secret)
        .map_err(AuthenticatedBridgeForwardError::Handshake)?;
    on_authenticated();
    forward_native_host_stream(browser_reader, browser_writer, agent_pipe)
        .map_err(AuthenticatedBridgeForwardError::Forward)
}

#[cfg(test)]
pub(crate) fn forward_authenticated_native_host_stream_with_log_and_authenticated_hook(
    browser_reader: &mut impl Read,
    browser_writer: &mut impl Write,
    agent_pipe: &mut (impl Read + Write),
    hello: &BridgeHello,
    secret: &PipeSecret,
    bridge_log: Option<&BridgeForwardLogSink>,
    mut on_authenticated: impl FnMut(),
) -> Result<(), AuthenticatedBridgeForwardError> {
    authenticate_client_pipe(agent_pipe, hello, secret)
        .map_err(AuthenticatedBridgeForwardError::Handshake)?;
    on_authenticated();
    forward_native_host_stream_with_log(browser_reader, browser_writer, agent_pipe, bridge_log)
        .map_err(AuthenticatedBridgeForwardError::Forward)
}

#[cfg(any(windows, test))]
pub(crate) fn forward_authenticated_native_host_stream_duplex_with_authenticated_hook<P>(
    browser_reader: &mut (impl Read + Send),
    browser_writer: &mut (impl Write + Send),
    mut agent_pipe: P,
    hello: &BridgeHello,
    secret: &PipeSecret,
    bridge_log: Option<BridgeForwardLogSink>,
    mut on_authenticated: impl FnMut(),
) -> Result<(), AuthenticatedBridgeForwardError>
where
    P: AuthenticatedDuplexAgentPipe,
{
    authenticate_client_pipe(&mut agent_pipe, hello, secret)
        .map_err(AuthenticatedBridgeForwardError::Handshake)?;
    on_authenticated();
    let (agent_reader, agent_writer, shutdown) = agent_pipe
        .split_for_duplex()
        .map_err(AuthenticatedBridgeForwardError::Split)?;
    let bridge_log = bridge_log;
    forward_native_host_stream_duplex_with_shutdown(
        browser_reader,
        browser_writer,
        agent_reader,
        agent_writer,
        shutdown,
        bridge_log.as_ref(),
    )
    .map_err(AuthenticatedBridgeForwardError::Forward)
}
