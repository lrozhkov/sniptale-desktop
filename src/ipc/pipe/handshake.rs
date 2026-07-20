//! Authenticated bridge handshake over the resident-agent pipe.

use std::io::{Read, Write};
use std::{error::Error, fmt};

use serde::{Serialize, de::DeserializeOwned};

#[cfg(any(windows, test))]
use super::client::build_auth_response;
#[cfg(any(windows, test))]
use super::server::PipeSecret;
#[cfg(test)]
use super::server::{AuthenticatedPipeSession, PipeServerAuth};
use super::server::{BridgeAuthResponse, BridgeHello, PipeAuthError, PipeChallenge};
use crate::ipc::native_messaging::{NativeMessagingError, read_payload, write_payload};

const PIPE_AUTH_FRAME_MAX_BYTES: usize = 16 * 1024;

#[derive(Debug)]
pub(crate) enum PipeHandshakeError {
    Frame(NativeMessagingError),
    Json(serde_json::Error),
    Auth(PipeAuthError),
    MissingFrame,
}

impl fmt::Display for PipeHandshakeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Frame(error) => write!(formatter, "pipe auth frame failed: {error}"),
            Self::Json(error) => write!(formatter, "pipe auth JSON failed: {error}"),
            Self::Auth(error) => write!(formatter, "pipe auth rejected: {error:?}"),
            Self::MissingFrame => formatter.write_str("pipe auth stream ended before handshake"),
        }
    }
}

impl Error for PipeHandshakeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Frame(error) => Some(error),
            Self::Json(error) => Some(error),
            Self::Auth(_) | Self::MissingFrame => None,
        }
    }
}

#[cfg(any(windows, test))]
pub(crate) fn authenticate_client_pipe(
    pipe: &mut (impl Read + Write),
    hello: &BridgeHello,
    secret: &PipeSecret,
) -> Result<(), PipeHandshakeError> {
    write_json_frame(pipe, hello)?;
    let challenge: PipeChallenge =
        read_json_frame(pipe)?.ok_or(PipeHandshakeError::MissingFrame)?;
    let response = build_auth_response(&challenge, secret).map_err(PipeHandshakeError::Auth)?;
    write_json_frame(pipe, &response)
}

#[cfg(test)]
pub(crate) fn authenticate_server_pipe(
    pipe: &mut (impl Read + Write),
    auth: &mut PipeServerAuth,
    now_ms: u64,
    ttl_ms: u64,
    session_id: &str,
    server_nonce: &str,
) -> Result<AuthenticatedPipeSession, PipeHandshakeError> {
    let hello: BridgeHello = read_json_frame(pipe)?.ok_or(PipeHandshakeError::MissingFrame)?;
    let challenge = auth
        .issue_challenge(hello, now_ms, ttl_ms, session_id, server_nonce)
        .map_err(PipeHandshakeError::Auth)?;
    if let Err(error) = write_json_frame(pipe, &challenge) {
        auth.discard_challenge(session_id);
        return Err(error);
    }
    let response = match read_json_frame(pipe) {
        Ok(Some(response)) => response,
        Ok(None) => {
            auth.discard_challenge(session_id);
            return Err(PipeHandshakeError::MissingFrame);
        }
        Err(error) => {
            auth.discard_challenge(session_id);
            return Err(error);
        }
    };
    match auth
        .authenticate(response, now_ms.saturating_add(1))
        .map_err(PipeHandshakeError::Auth)
    {
        Ok(session) => Ok(session),
        Err(error) => {
            auth.discard_challenge(session_id);
            Err(error)
        }
    }
}

pub(crate) fn read_server_pipe_hello(
    pipe: &mut impl Read,
) -> Result<BridgeHello, PipeHandshakeError> {
    read_json_frame(pipe)?.ok_or(PipeHandshakeError::MissingFrame)
}

pub(crate) fn write_server_pipe_challenge(
    pipe: &mut impl Write,
    challenge: &PipeChallenge,
) -> Result<(), PipeHandshakeError> {
    write_json_frame(pipe, challenge)
}

pub(crate) fn read_server_pipe_auth_response(
    pipe: &mut impl Read,
) -> Result<BridgeAuthResponse, PipeHandshakeError> {
    read_json_frame(pipe)?.ok_or(PipeHandshakeError::MissingFrame)
}

fn read_json_frame<T: DeserializeOwned>(
    pipe: &mut impl Read,
) -> Result<Option<T>, PipeHandshakeError> {
    let Some(payload) =
        read_payload(pipe, PIPE_AUTH_FRAME_MAX_BYTES).map_err(PipeHandshakeError::Frame)?
    else {
        return Ok(None);
    };
    serde_json::from_slice(&payload)
        .map(Some)
        .map_err(PipeHandshakeError::Json)
}

fn write_json_frame<T: Serialize>(
    pipe: &mut impl Write,
    value: &T,
) -> Result<(), PipeHandshakeError> {
    let payload = serde_json::to_vec(value).map_err(PipeHandshakeError::Json)?;
    write_payload(pipe, &payload, PIPE_AUTH_FRAME_MAX_BYTES).map_err(PipeHandshakeError::Frame)
}
