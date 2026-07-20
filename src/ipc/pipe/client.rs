//! Native-host bridge client for the resident agent pipe.

#[cfg(all(windows, not(test)))]
use std::{fmt, io};

#[cfg(any(windows, test))]
use super::{
    auth_mac::challenge_response_hash,
    server::{BridgeAuthResponse, PipeAuthError, PipeChallenge, PipeSecret},
};
#[cfg(all(windows, not(test)))]
use super::{name::PipeEndpointName, windows_pipe::WindowsPipe};

#[cfg(any(windows, test))]
pub(crate) fn build_auth_response(
    challenge: &PipeChallenge,
    secret: &PipeSecret,
) -> Result<BridgeAuthResponse, PipeAuthError> {
    Ok(BridgeAuthResponse {
        algorithm: challenge.algorithm.clone(),
        session_id: challenge.session_id.clone(),
        client_nonce: challenge.client_nonce.clone(),
        server_nonce: challenge.server_nonce.clone(),
        response_hash: challenge_response_hash(secret, &challenge.install_id, challenge)?,
    })
}

#[cfg(all(windows, not(test)))]
#[derive(Debug)]
pub(crate) enum ResidentPipeConnectError {
    InvalidInstallId,
    NotFound,
    Busy,
    PermissionDenied,
    Other(io::Error),
}

#[cfg(all(windows, not(test)))]
impl ResidentPipeConnectError {
    pub(crate) fn should_start_resident(&self) -> bool {
        matches!(self, Self::NotFound)
    }
}

#[cfg(all(windows, not(test)))]
impl fmt::Display for ResidentPipeConnectError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInstallId => formatter.write_str("resident pipe install id is invalid"),
            Self::NotFound => formatter.write_str("resident pipe was not found"),
            Self::Busy => formatter.write_str("resident pipe is busy"),
            Self::PermissionDenied => formatter.write_str("resident pipe permission denied"),
            Self::Other(error) => write!(formatter, "resident pipe connection failed: {error}"),
        }
    }
}

#[cfg(all(windows, not(test)))]
impl std::error::Error for ResidentPipeConnectError {}

#[cfg(all(windows, not(test)))]
pub(crate) fn connect_resident_pipe(
    install_id: &str,
) -> Result<WindowsPipe, ResidentPipeConnectError> {
    let endpoint = PipeEndpointName::for_install(install_id)
        .map_err(|_| ResidentPipeConnectError::InvalidInstallId)?;
    WindowsPipe::connect_client(&endpoint).map_err(classify_connect_error)
}

#[cfg(all(windows, not(test)))]
fn classify_connect_error(error: io::Error) -> ResidentPipeConnectError {
    match error.raw_os_error() {
        Some(2 | 3) => ResidentPipeConnectError::NotFound,
        Some(5) => ResidentPipeConnectError::PermissionDenied,
        Some(231) => ResidentPipeConnectError::Busy,
        _ if error.kind() == io::ErrorKind::NotFound => ResidentPipeConnectError::NotFound,
        _ if error.kind() == io::ErrorKind::PermissionDenied => {
            ResidentPipeConnectError::PermissionDenied
        }
        _ => ResidentPipeConnectError::Other(error),
    }
}
