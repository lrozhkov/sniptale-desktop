//! Transfer-owned browser-facing media chunk message builders.

use std::{error::Error, fmt};

use serde_json::Value;

use super::chunks::EncodedChunk;
use crate::protocol::{
    AppRecordingChunkMessage, AppScreenshotChunkMessage, AppScreenshotCommitMessage,
    PROTOCOL_VERSION, WireMessage,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TransferChunkMessageError {
    InvalidChunkIndex,
    Serialization,
}

impl fmt::Display for TransferChunkMessageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidChunkIndex => formatter.write_str("transfer chunk index is invalid"),
            Self::Serialization => formatter.write_str("transfer chunk serialization failed"),
        }
    }
}

impl Error for TransferChunkMessageError {}

pub(crate) fn screenshot_chunk_value(
    controller_lease_id: &str,
    capture_id: &str,
    chunk: EncodedChunk,
) -> Result<Value, TransferChunkMessageError> {
    let chunk_index =
        u32::try_from(chunk.index).map_err(|_| TransferChunkMessageError::InvalidChunkIndex)?;
    wire_value(WireMessage::AppScreenshotChunk(Box::new(
        AppScreenshotChunkMessage {
            protocol_version: PROTOCOL_VERSION,
            controller_lease_id: controller_lease_id.to_owned(),
            capture_id: capture_id.to_owned(),
            chunk_index,
            chunk_byte_offset: chunk.byte_offset,
            chunk_raw_bytes: chunk.raw_bytes,
            chunk_sha256: chunk.chunk_sha256,
            base64: chunk.base64,
        },
    )))
}

pub(crate) fn screenshot_commit_value(
    controller_lease_id: &str,
    capture_id: &str,
) -> Result<Value, TransferChunkMessageError> {
    wire_value(WireMessage::AppScreenshotCommit(Box::new(
        AppScreenshotCommitMessage {
            protocol_version: PROTOCOL_VERSION,
            controller_lease_id: controller_lease_id.to_owned(),
            capture_id: capture_id.to_owned(),
        },
    )))
}

pub(crate) fn recording_chunk_value(
    controller_lease_id: &str,
    recording_id: &str,
    chunk: EncodedChunk,
) -> Result<Value, TransferChunkMessageError> {
    let chunk_index =
        u32::try_from(chunk.index).map_err(|_| TransferChunkMessageError::InvalidChunkIndex)?;
    wire_value(WireMessage::AppRecordingChunk(Box::new(
        AppRecordingChunkMessage {
            protocol_version: PROTOCOL_VERSION,
            controller_lease_id: controller_lease_id.to_owned(),
            recording_id: recording_id.to_owned(),
            chunk_index,
            chunk_byte_offset: chunk.byte_offset,
            chunk_raw_bytes: chunk.raw_bytes,
            chunk_sha256: chunk.chunk_sha256,
            base64: chunk.base64,
        },
    )))
}

fn wire_value(message: WireMessage) -> Result<Value, TransferChunkMessageError> {
    serde_json::to_value(message).map_err(|_| TransferChunkMessageError::Serialization)
}
