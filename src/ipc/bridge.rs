#[cfg(any(windows, test))]
mod authenticated;
#[cfg(any(windows, test))]
mod duplex;
#[cfg(any(windows, test))]
mod forwarding;
#[cfg(any(windows, test))]
mod logging;
#[cfg(test)]
mod polling;
mod response_header;
#[cfg(any(windows, test))]
mod shutdown;

#[cfg(any(windows, test))]
use std::error::Error;
use std::fmt;
#[cfg(test)]
use std::io::{Read, Write};

#[cfg(test)]
use crate::protocol::EXTENSION_TO_HOST_MAX_MESSAGE_BYTES;

#[cfg(any(windows, test))]
pub(crate) use self::authenticated::AuthenticatedBridgeForwardError;
#[cfg(test)]
pub(crate) use self::authenticated::forward_authenticated_native_host_stream;
#[cfg(test)]
pub(crate) use self::authenticated::forward_authenticated_native_host_stream_with_authenticated_hook;
#[cfg(test)]
pub(crate) use self::authenticated::forward_authenticated_native_host_stream_with_log_and_authenticated_hook;
#[cfg(any(windows, test))]
pub(crate) use self::authenticated::{
    AuthenticatedDuplexAgentPipe,
    forward_authenticated_native_host_stream_duplex_with_authenticated_hook,
};
#[cfg(test)]
pub(crate) use self::duplex::forward_native_host_stream_duplex;
#[cfg(any(windows, test))]
pub(crate) use self::duplex::forward_native_host_stream_duplex_with_shutdown;
#[cfg(test)]
use self::forwarding::forward_browser_payload;
#[cfg(test)]
use self::logging::bridge_error_forward_phase;
#[cfg(test)]
use self::logging::log_bridge_error;
#[cfg(any(windows, test))]
pub(crate) use self::logging::{BridgeForwardLog, BridgeForwardLogSink};
#[cfg(test)]
pub(crate) use self::polling::{
    AgentPipeResponseAvailability, forward_native_host_stream_polling_agent_with_shutdown,
};
#[cfg(any(windows, test))]
use self::response_header::parse_agent_pipe_response_header;
pub(crate) use self::response_header::{
    AGENT_PIPE_RESPONSE_HEADER_MAX_BYTES, AgentPipeResponseHeaderError,
    agent_pipe_response_header_payload,
};
#[cfg(any(windows, test))]
use super::NativeMessagingError;
#[cfg(test)]
use super::read_payload;

pub(crate) trait AgentPipeReadShutdown: Send {
    fn shutdown_agent_read(&self);
}

#[cfg(any(windows, test))]
#[derive(Debug)]
pub(crate) enum BridgeForwardError {
    BrowserFrame(NativeMessagingError),
    AgentFrame(NativeMessagingError),
    BrowserProtocol(crate::protocol::ParseError),
    AgentProtocol(crate::protocol::ParseError),
    AgentResponseHeader(AgentPipeResponseHeaderError),
    AgentResponseMissing,
    ForwardThread,
}

#[cfg(any(windows, test))]
impl fmt::Display for BridgeForwardError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BrowserFrame(error) => write!(formatter, "browser frame failed: {error}"),
            Self::AgentFrame(error) => write!(formatter, "agent pipe frame failed: {error}"),
            Self::BrowserProtocol(error) => write!(formatter, "browser payload rejected: {error}"),
            Self::AgentProtocol(error) => write!(formatter, "agent payload rejected: {error}"),
            Self::AgentResponseHeader(error) => {
                write!(formatter, "agent response header rejected: {error}")
            }
            Self::AgentResponseMissing => formatter.write_str("agent response frame missing"),
            Self::ForwardThread => formatter.write_str("bridge forwarding thread failed"),
        }
    }
}

#[cfg(any(windows, test))]
impl Error for BridgeForwardError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::BrowserFrame(error) | Self::AgentFrame(error) => Some(error),
            Self::BrowserProtocol(error) | Self::AgentProtocol(error) => Some(error),
            Self::AgentResponseHeader(error) => Some(error),
            Self::AgentResponseMissing | Self::ForwardThread => None,
        }
    }
}

#[cfg(test)]
pub(crate) fn forward_native_host_stream(
    browser_reader: &mut impl Read,
    browser_writer: &mut impl Write,
    agent_pipe: &mut (impl Read + Write),
) -> Result<(), BridgeForwardError> {
    forward_native_host_stream_with_log(browser_reader, browser_writer, agent_pipe, None)
}

#[cfg(test)]
pub(crate) fn forward_native_host_stream_with_log(
    browser_reader: &mut impl Read,
    browser_writer: &mut impl Write,
    agent_pipe: &mut (impl Read + Write),
    log: Option<&BridgeForwardLogSink>,
) -> Result<(), BridgeForwardError> {
    loop {
        let payload = match read_payload(browser_reader, EXTENSION_TO_HOST_MAX_MESSAGE_BYTES) {
            Ok(Some(payload)) => payload,
            Ok(None) => return Ok(()),
            Err(error) => {
                let error = BridgeForwardError::BrowserFrame(error);
                log_bridge_error(log, "browser-to-agent-forward-error", &error);
                return Err(error);
            }
        };
        if let Err(error) = forward_browser_payload(&payload, browser_writer, agent_pipe, log) {
            log_bridge_error(log, bridge_error_forward_phase(&error), &error);
            return Err(error);
        }
    }
}
