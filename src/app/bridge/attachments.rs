//! Native-host-only protected attachment reads after the resident's exact authorization grant.

use crate::{
    ipc::{AgentAttachmentReader, BridgeForwardError},
    protocol::{
        WireMessage,
        recording_events::{AppRecordingEventsChunkMessage, RecordingEventsReadGrant},
        trajectory::{AppRecordingAttachmentChunkMessage, CursorTrajectoryReadGrant},
    },
    transfer::trajectory_store::{TrajectoryArchive, default_trajectory_root},
};
use std::path::PathBuf;

#[derive(Default)]
pub(super) struct NativeTrajectoryAttachmentReader {
    root: Option<PathBuf>,
    events_root: Option<PathBuf>,
}

impl std::fmt::Debug for NativeTrajectoryAttachmentReader {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("NativeTrajectoryAttachmentReader")
            .finish_non_exhaustive()
    }
}

impl AgentAttachmentReader for NativeTrajectoryAttachmentReader {
    fn read_events(
        &mut self,
        grant: &RecordingEventsReadGrant,
    ) -> Result<WireMessage, BridgeForwardError> {
        let root = self
            .events_root
            .clone()
            .map_or_else(crate::transfer::events_store::default_events_root, Ok)
            .map_err(|_| BridgeForwardError::AttachmentRead)?;
        let mut archive = crate::transfer::events_store::EventsArchive::read(
            &root,
            &grant.recording_id,
            &grant.event_stream_id,
        )
        .map_err(|_| BridgeForwardError::AttachmentRead)?;
        let chunk = archive
            .chunk(grant.sequence, grant.chunk_index)
            .map_err(|_| BridgeForwardError::AttachmentRead)?;
        if chunk.block_bytes != grant.block_bytes || chunk.block_sha256 != grant.block_sha256 {
            return Err(BridgeForwardError::AttachmentRead);
        }
        Ok(WireMessage::AppRecordingEventsChunk(Box::new(
            AppRecordingEventsChunkMessage {
                protocol_version: grant.protocol_version,
                controller_lease_id: grant.controller_lease_id.clone(),
                recording_id: grant.recording_id.clone(),
                event_stream_id: grant.event_stream_id.clone(),
                request_id: grant.request_id.clone(),
                sequence: chunk.sequence,
                block_bytes: chunk.block_bytes,
                block_sha256: chunk.block_sha256,
                chunk_index: chunk.chunk_index,
                chunk_count: chunk.chunk_count,
                chunk_byte_offset: chunk.chunk_byte_offset,
                chunk_raw_bytes: chunk.chunk_raw_bytes,
                chunk_sha256: chunk.chunk_sha256,
                base64: chunk.base64,
            },
        )))
    }
    fn read(
        &mut self,
        grant: &CursorTrajectoryReadGrant,
    ) -> Result<WireMessage, BridgeForwardError> {
        let root = self
            .root
            .clone()
            .map_or_else(default_trajectory_root, Ok)
            .map_err(|_| BridgeForwardError::AttachmentRead)?;
        let mut archive = TrajectoryArchive::read(&root, &grant.recording_id, &grant.trajectory_id)
            .map_err(|_| BridgeForwardError::AttachmentRead)?;
        let chunk = archive
            .chunk(grant.sequence, grant.chunk_index)
            .map_err(|_| BridgeForwardError::AttachmentRead)?;
        if chunk.block_bytes != grant.block_bytes || chunk.block_sha256 != grant.block_sha256 {
            return Err(BridgeForwardError::AttachmentRead);
        }
        Ok(WireMessage::AppRecordingAttachmentChunk(Box::new(
            AppRecordingAttachmentChunkMessage {
                protocol_version: grant.protocol_version,
                controller_lease_id: grant.controller_lease_id.clone(),
                recording_id: grant.recording_id.clone(),
                trajectory_id: grant.trajectory_id.clone(),
                request_id: grant.request_id.clone(),
                sequence: chunk.sequence,
                block_bytes: chunk.block_bytes,
                block_sha256: chunk.block_sha256,
                chunk_index: chunk.chunk_index,
                chunk_count: chunk.chunk_count,
                chunk_byte_offset: chunk.chunk_byte_offset,
                chunk_raw_bytes: chunk.chunk_raw_bytes,
                chunk_sha256: chunk.chunk_sha256,
                base64: chunk.base64,
            },
        )))
    }
}

#[cfg(test)]
mod tests;
