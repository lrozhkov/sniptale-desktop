//! Current-user resident-pipe authentication data types.

use std::{error::Error, fmt};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum PipeAuthError {
    MissingSecret,
    WrongUser,
    WrongInstall,
    WrongOrigin,
    InvalidNonce,
    ExpiredChallenge,
    ReplayedChallenge,
    InvalidResponse,
    SessionLimit,
    QueueFull,
}

impl fmt::Display for PipeAuthError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingSecret => formatter.write_str("missing pipe secret"),
            Self::WrongUser => formatter.write_str("pipe user mismatch"),
            Self::WrongInstall => formatter.write_str("pipe install mismatch"),
            Self::WrongOrigin => formatter.write_str("pipe extension origin mismatch"),
            Self::InvalidNonce => formatter.write_str("invalid pipe nonce"),
            Self::ExpiredChallenge => formatter.write_str("expired pipe challenge"),
            Self::ReplayedChallenge => formatter.write_str("replayed pipe challenge"),
            Self::InvalidResponse => formatter.write_str("invalid pipe auth response"),
            Self::SessionLimit => formatter.write_str("pipe session limit reached"),
            Self::QueueFull => formatter.write_str("pipe session queue is full"),
        }
    }
}

impl Error for PipeAuthError {}

#[derive(Clone, Eq, PartialEq)]
pub(crate) struct PipeSecret(pub(super) String);

impl std::fmt::Debug for PipeSecret {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("PipeSecret(<redacted>)")
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BridgeHello {
    pub(crate) current_user_sid: String,
    pub(crate) install_id: String,
    pub(crate) extension_origin: String,
    pub(crate) client_nonce: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PipeChallenge {
    pub(crate) algorithm: String,
    pub(crate) session_id: String,
    pub(crate) install_id: String,
    pub(crate) client_nonce: String,
    pub(crate) server_nonce: String,
    pub(crate) expires_at_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BridgeAuthResponse {
    pub(crate) algorithm: String,
    pub(crate) session_id: String,
    pub(crate) client_nonce: String,
    pub(crate) server_nonce: String,
    pub(crate) response_hash: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AuthenticatedPipeSession {
    pub(crate) session_id: String,
    pub(crate) current_user_sid: String,
    pub(crate) extension_origin: String,
    pub(super) queue_capacity: usize,
    pub(super) queued_messages: usize,
}
