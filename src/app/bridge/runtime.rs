//! Native-host bridge runtime entrypoints.

use std::io::{Read, Write};

use super::NativeHostRunError;
#[cfg(any(not(windows), test))]
use super::local::run_local_response_bridge;
#[cfg(all(windows, not(test)))]
use super::windows_runtime::run_pipe_forwarding_bridge;
#[cfg(any(windows, test))]
use crate::ipc::PipeSecret;
#[cfg(all(windows, not(test)))]
use crate::ipc::{
    AuthenticatedDuplexAgentPipe, BridgeForwardLogSink, BridgeHello,
    forward_authenticated_native_host_stream_duplex_with_authenticated_hook,
};
#[cfg(test)]
use crate::ipc::{BridgeHello, forward_authenticated_native_host_stream};

pub(crate) trait NativeHostRuntimeLog {
    #[cfg(all(windows, not(test)))]
    fn phase(&self, phase: &'static str, message: &str);

    #[cfg(all(windows, not(test)))]
    fn bridge_forward_log(&self) -> Option<BridgeForwardLogSink> {
        None
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(any(windows, test))]
pub(crate) struct NativeHostPipeAuthConfig {
    pub(crate) current_user_sid: String,
    pub(crate) install_id: String,
    pub(crate) extension_origin: String,
    pub(crate) client_nonce: String,
    pub(crate) secret: PipeSecret,
}

#[cfg(all(windows, not(test)))]
pub(crate) fn run_native_host_bridge(
    reader: &mut (impl Read + Send),
    writer: &mut (impl Write + Send),
    extension_origin: &str,
    log: Option<&dyn NativeHostRuntimeLog>,
) -> Result<(), NativeHostRunError> {
    run_pipe_forwarding_bridge(reader, writer, extension_origin, log)
}

#[cfg(any(not(windows), test))]
pub(crate) fn run_native_host_bridge(
    reader: &mut (impl Read + Send),
    writer: &mut (impl Write + Send),
    _extension_origin: &str,
    _log: Option<&dyn NativeHostRuntimeLog>,
) -> Result<(), NativeHostRunError> {
    run_local_response_bridge(reader, writer)
}

#[cfg(test)]
pub(crate) fn run_authenticated_pipe_forwarding_bridge(
    reader: &mut impl Read,
    writer: &mut impl Write,
    agent_pipe: &mut (impl Read + Write),
    config: &NativeHostPipeAuthConfig,
) -> Result<(), NativeHostRunError> {
    let hello = BridgeHello {
        current_user_sid: config.current_user_sid.clone(),
        install_id: config.install_id.clone(),
        extension_origin: config.extension_origin.clone(),
        client_nonce: config.client_nonce.clone(),
    };
    forward_authenticated_native_host_stream(reader, writer, agent_pipe, &hello, &config.secret)
        .map_err(NativeHostRunError::AuthenticatedBridge)
}

#[cfg(all(windows, not(test)))]
pub(crate) fn run_authenticated_pipe_forwarding_bridge_with_log(
    reader: &mut (impl Read + Send),
    writer: &mut (impl Write + Send),
    agent_pipe: impl AuthenticatedDuplexAgentPipe,
    config: &NativeHostPipeAuthConfig,
    log: Option<&dyn NativeHostRuntimeLog>,
) -> Result<(), NativeHostRunError> {
    let hello = BridgeHello {
        current_user_sid: config.current_user_sid.clone(),
        install_id: config.install_id.clone(),
        extension_origin: config.extension_origin.clone(),
        client_nonce: config.client_nonce.clone(),
    };
    let bridge_log = log.and_then(NativeHostRuntimeLog::bridge_forward_log);
    forward_authenticated_native_host_stream_duplex_with_authenticated_hook(
        reader,
        writer,
        agent_pipe,
        &hello,
        &config.secret,
        bridge_log,
        || {
            if let Some(log) = log {
                log.phase("pipe-authenticated", "resident agent pipe authenticated");
                log.phase(
                    "forwarding-duplex",
                    "forwarding native messaging frames with resident blocking duplex bridge",
                );
            }
        },
    )
    .map_err(NativeHostRunError::AuthenticatedBridge)
}

#[cfg(all(windows, not(test)))]
pub(super) fn bridge_client_nonce() -> Result<String, crate::ipc::WindowsPipeEntropyError> {
    crate::ipc::random_pipe_auth_bytes::<16>().map(pipe_auth_nonce_from_bytes)
}

#[cfg(any(windows, test))]
fn pipe_auth_nonce_from_bytes(bytes: [u8; 16]) -> String {
    format!(
        "client-nonce-{}",
        crate::transfer::hashing::sha256_hex(&bytes)
    )
}

#[cfg(test)]
mod tests {
    use super::pipe_auth_nonce_from_bytes;

    #[test]
    fn pipe_auth_nonce_is_derived_from_random_bytes_not_pid_time() {
        let nonce = pipe_auth_nonce_from_bytes([0xab; 16]);

        assert!(nonce.starts_with("client-nonce-"));
        let digest = &nonce["client-nonce-".len()..];
        assert_eq!(digest.len(), 64);
        assert!(
            digest
                .chars()
                .all(|character| character.is_ascii_hexdigit())
        );
    }
}
