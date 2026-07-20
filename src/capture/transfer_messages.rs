//! Screenshot transfer protocol message builders for finalized PNG files.

#[cfg(test)]
use std::path::Path;

use super::coordinates::PhysicalRect;
#[cfg(test)]
use crate::protocol::AppScreenshotChunkMessage;
#[cfg(test)]
use crate::protocol::AppScreenshotCommitMessage;
#[cfg(test)]
use crate::transfer::{chunks::MediaTransferKind, file::encode_file_chunk};
use crate::{
    protocol::{AppScreenshotStartMessage, PROTOCOL_VERSION, ScreenshotMimeType, ScreenshotMode},
    transfer::{chunks::chunk_count, error::TransferError},
};

#[cfg(test)]
const SCREENSHOT_CHUNK_ENVELOPE_UTF8_BYTES: usize = 512;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ScreenshotTransferSessionInput {
    pub(crate) controller_lease_id: String,
    pub(crate) capture_id: String,
    pub(crate) mode: ScreenshotMode,
    pub(crate) filename: String,
    pub(crate) bounds: PhysicalRect,
    pub(crate) total_bytes: u64,
    pub(crate) sha256: String,
    pub(crate) open_editor: bool,
    pub(crate) captured_at_epoch_ms: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ScreenshotTransferSession {
    pub(crate) controller_lease_id: String,
    pub(crate) capture_id: String,
    pub(crate) mode: ScreenshotMode,
    pub(crate) filename: String,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) total_bytes: u64,
    pub(crate) sha256: String,
    pub(crate) chunk_count: u32,
    pub(crate) open_editor: bool,
    pub(crate) captured_at_epoch_ms: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ScreenshotTransferMessageError {
    InvalidChunkCount,
    Transfer(TransferError),
}

pub(crate) fn screenshot_transfer_session(
    input: ScreenshotTransferSessionInput,
) -> Result<ScreenshotTransferSession, ScreenshotTransferMessageError> {
    let chunks =
        chunk_count(input.total_bytes).map_err(ScreenshotTransferMessageError::Transfer)?;
    Ok(ScreenshotTransferSession {
        controller_lease_id: input.controller_lease_id,
        capture_id: input.capture_id,
        mode: input.mode,
        filename: input.filename,
        width: input.bounds.width,
        height: input.bounds.height,
        total_bytes: input.total_bytes,
        sha256: input.sha256,
        chunk_count: u32::try_from(chunks)
            .map_err(|_| ScreenshotTransferMessageError::InvalidChunkCount)?,
        open_editor: input.open_editor,
        captured_at_epoch_ms: input.captured_at_epoch_ms,
    })
}

pub(crate) fn screenshot_start_message(
    session: &ScreenshotTransferSession,
) -> AppScreenshotStartMessage {
    AppScreenshotStartMessage {
        protocol_version: PROTOCOL_VERSION,
        controller_lease_id: session.controller_lease_id.clone(),
        capture_id: session.capture_id.clone(),
        mode: session.mode.clone(),
        filename: session.filename.clone(),
        mime_type: ScreenshotMimeType::ImagePng,
        total_bytes: session.total_bytes,
        width: session.width,
        height: session.height,
        chunk_count: session.chunk_count,
        sha256: session.sha256.clone(),
        open_editor: session.open_editor,
        captured_at_epoch_ms: session.captured_at_epoch_ms,
    }
}

#[cfg(test)]
pub(crate) fn screenshot_chunk_message(
    path: &Path,
    session: &ScreenshotTransferSession,
    chunk_index: u32,
) -> Result<AppScreenshotChunkMessage, ScreenshotTransferMessageError> {
    let chunk = encode_file_chunk(
        MediaTransferKind::Screenshot,
        path,
        session.total_bytes,
        u64::from(chunk_index),
        SCREENSHOT_CHUNK_ENVELOPE_UTF8_BYTES,
    )
    .map_err(ScreenshotTransferMessageError::Transfer)?;
    Ok(AppScreenshotChunkMessage {
        protocol_version: PROTOCOL_VERSION,
        controller_lease_id: session.controller_lease_id.clone(),
        capture_id: session.capture_id.clone(),
        chunk_index,
        chunk_byte_offset: chunk.byte_offset,
        chunk_raw_bytes: chunk.raw_bytes,
        chunk_sha256: chunk.chunk_sha256,
        base64: chunk.base64,
    })
}

#[cfg(test)]
pub(crate) fn screenshot_commit_message(
    session: &ScreenshotTransferSession,
) -> AppScreenshotCommitMessage {
    AppScreenshotCommitMessage {
        protocol_version: PROTOCOL_VERSION,
        controller_lease_id: session.controller_lease_id.clone(),
        capture_id: session.capture_id.clone(),
    }
}
