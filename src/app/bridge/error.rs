//! Native-host bridge run errors.

use std::{error::Error, fmt};

#[cfg(any(windows, test))]
use crate::ipc::AuthenticatedBridgeForwardError;

#[derive(Debug)]
pub(crate) enum NativeHostRunError {
    #[cfg(any(not(windows), test))]
    Frame(crate::ipc::NativeMessagingError),
    #[cfg(any(not(windows), test))]
    Protocol(crate::protocol::ParseError),
    #[cfg(any(windows, test))]
    AuthenticatedBridge(AuthenticatedBridgeForwardError),
    #[cfg(all(windows, not(test)))]
    InstallIdentityDrift(crate::install::ManifestError),
    #[cfg(all(windows, not(test)))]
    PipeAuthenticationUnavailable,
    #[cfg(any(not(windows), test))]
    Json(serde_json::Error),
}

impl fmt::Display for NativeHostRunError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            #[cfg(any(not(windows), test))]
            Self::Frame(error) => write!(formatter, "native messaging frame failed: {error}"),
            #[cfg(any(not(windows), test))]
            Self::Protocol(error) => {
                write!(formatter, "native messaging protocol rejected: {error}")
            }
            #[cfg(any(windows, test))]
            Self::AuthenticatedBridge(error) => {
                write!(formatter, "authenticated bridge failed: {error}")
            }
            #[cfg(all(windows, not(test)))]
            Self::InstallIdentityDrift(error) => {
                write!(formatter, "native host install identity drift: {error}")
            }
            #[cfg(all(windows, not(test)))]
            Self::PipeAuthenticationUnavailable => {
                formatter.write_str("pipe authentication is unavailable")
            }
            #[cfg(any(not(windows), test))]
            Self::Json(error) => write!(formatter, "native messaging JSON failed: {error}"),
        }
    }
}

impl Error for NativeHostRunError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            #[cfg(any(not(windows), test))]
            Self::Frame(error) => Some(error),
            #[cfg(any(not(windows), test))]
            Self::Protocol(error) => Some(error),
            #[cfg(any(windows, test))]
            Self::AuthenticatedBridge(error) => Some(error),
            #[cfg(all(windows, not(test)))]
            Self::InstallIdentityDrift(_) => None,
            #[cfg(all(windows, not(test)))]
            Self::PipeAuthenticationUnavailable => None,
            #[cfg(any(not(windows), test))]
            Self::Json(error) => Some(error),
        }
    }
}
