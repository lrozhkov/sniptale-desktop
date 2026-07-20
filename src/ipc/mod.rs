//! Native Messaging and agent pipe framing.

#[cfg(any(windows, test))]
mod bridge;
mod controller_lease;
mod native_messaging;
#[cfg(any(windows, test))]
mod pipe;
#[cfg(test)]
mod tests;

#[cfg(any(windows, test))]
pub(crate) use bridge::AuthenticatedBridgeForwardError;
#[cfg(test)]
pub(crate) use bridge::forward_authenticated_native_host_stream;
#[cfg(test)]
pub(crate) use bridge::forward_authenticated_native_host_stream_with_authenticated_hook;
#[cfg(test)]
pub(crate) use bridge::forward_authenticated_native_host_stream_with_log_and_authenticated_hook;
#[cfg(test)]
pub(crate) use bridge::forward_native_host_stream;
#[cfg(any(windows, test))]
pub(crate) use bridge::{
    AGENT_PIPE_RESPONSE_HEADER_MAX_BYTES, AgentPipeReadShutdown, AgentPipeResponseHeaderError,
    agent_pipe_response_header_payload,
};
#[cfg(any(windows, test))]
pub(crate) use bridge::{
    AuthenticatedDuplexAgentPipe,
    forward_authenticated_native_host_stream_duplex_with_authenticated_hook,
};
#[cfg(test)]
pub(crate) use bridge::{BridgeForwardError, forward_native_host_stream_duplex};
#[cfg(any(windows, test))]
pub(crate) use bridge::{BridgeForwardLog, BridgeForwardLogSink};
pub(crate) use controller_lease::{
    ControllerLeaseBook, ControllerLeaseDecision, ControllerLeaseError,
};
pub(crate) use native_messaging::{NativeMessagingError, read_payload, write_payload};
#[cfg(test)]
pub(crate) use pipe::client::build_auth_response;
#[cfg(all(windows, not(test)))]
pub(crate) use pipe::client::{ResidentPipeConnectError, connect_resident_pipe};
#[cfg(any(windows, test))]
pub(crate) use pipe::handshake::PipeHandshakeError;
#[cfg(any(windows, test))]
pub(crate) use pipe::handshake::authenticate_client_pipe;
#[cfg(test)]
pub(crate) use pipe::handshake::authenticate_server_pipe;
#[cfg(any(windows, test))]
pub(crate) use pipe::handshake::{
    read_server_pipe_auth_response, read_server_pipe_hello, write_server_pipe_challenge,
};
#[cfg(test)]
pub(crate) use pipe::name::{PipeEndpointName, PipeNameError};
#[cfg(test)]
pub(crate) use pipe::security::{PipeSecurityDescriptorError, current_user_pipe_sddl};
#[cfg(any(windows, test))]
pub(crate) use pipe::server::BridgeHello;
#[cfg(any(windows, test))]
pub(crate) use pipe::server::PipeSecret;
#[cfg(any(windows, test))]
pub(crate) use pipe::server::{AuthenticatedPipeSession, PipeAuthError, PipeServerAuth};
#[cfg(test)]
pub(crate) use pipe::server::{BridgeAuthResponse, PipeChallenge};
#[cfg(all(windows, not(test)))]
pub(crate) use pipe::server_windows::create_resident_agent_singleton_guard;
#[cfg(windows)]
pub(crate) use pipe::server_windows::{
    create_resident_pipe_listener, poll_resident_pipe_listener, set_resident_pipe_blocking,
};
#[cfg(all(windows, not(test)))]
pub(crate) use pipe::windows_entropy::WindowsPipeEntropyError;
#[cfg(windows)]
pub(crate) use pipe::windows_entropy::random_pipe_auth_bytes;
#[cfg(windows)]
pub(crate) use pipe::windows_pipe::WindowsPipe;
#[cfg(windows)]
pub(crate) use pipe::windows_pipe::{WindowsPipeReader, WindowsPipeShutdown, WindowsPipeWriter};
#[cfg(windows)]
pub(crate) use pipe::windows_pipe_peer::WindowsPipePeerIdentity;
