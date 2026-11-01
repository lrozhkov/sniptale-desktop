//! A narrow native-host port resolves authenticated control grants outside the pipe.

use super::{BridgeForwardError, BridgeForwardLogSink};
use crate::protocol::{
    WireMessage,
    recording_events::{RecordingEventsReadGrant, parse_events_read_grant},
    trajectory::{CursorTrajectoryReadGrant, parse_trajectory_read_grant},
};

pub(crate) trait AgentAttachmentReader: Send {
    fn read_events(
        &mut self,
        _grant: &RecordingEventsReadGrant,
    ) -> Result<WireMessage, BridgeForwardError> {
        Err(BridgeForwardError::AttachmentRead)
    }
    fn read(
        &mut self,
        grant: &CursorTrajectoryReadGrant,
    ) -> Result<WireMessage, BridgeForwardError>;
}

pub(crate) struct BridgeForwardRuntime {
    pub(crate) log: Option<BridgeForwardLogSink>,
    pub(crate) attachment_reader: Box<dyn AgentAttachmentReader>,
}

impl std::fmt::Debug for BridgeForwardRuntime {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("BridgeForwardRuntime")
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
impl BridgeForwardRuntime {
    pub(crate) fn without_attachments(log: Option<BridgeForwardLogSink>) -> Self {
        Self {
            log,
            attachment_reader: Box::new(RejectAttachmentReads),
        }
    }
}

#[cfg(test)]
struct RejectAttachmentReads;

#[cfg(test)]
impl AgentAttachmentReader for RejectAttachmentReads {
    fn read(
        &mut self,
        _grant: &CursorTrajectoryReadGrant,
    ) -> Result<WireMessage, BridgeForwardError> {
        Err(BridgeForwardError::AttachmentRead)
    }
}

pub(super) fn resolve_grant(
    bytes: &[u8],
    reader: &mut dyn AgentAttachmentReader,
) -> Result<Vec<u8>, BridgeForwardError> {
    if let Ok(grant) = parse_events_read_grant(bytes) {
        return resolve_events_grant(&grant, reader);
    }
    let grant = parse_trajectory_read_grant(bytes).map_err(BridgeForwardError::AgentProtocol)?;
    let message = reader.read(&grant)?;
    match &message {
        WireMessage::AppRecordingAttachmentChunk(value)
            if value.protocol_version == grant.protocol_version
                && value.controller_lease_id == grant.controller_lease_id
                && value.recording_id == grant.recording_id
                && value.trajectory_id == grant.trajectory_id
                && value.request_id == grant.request_id
                && value.sequence == grant.sequence
                && value.chunk_index == grant.chunk_index
                && value.block_bytes == grant.block_bytes
                && value.block_sha256 == grant.block_sha256 => {}
        _ => return Err(BridgeForwardError::AttachmentRead),
    }
    serde_json::to_vec(&message).map_err(|_| BridgeForwardError::AttachmentRead)
}

fn resolve_events_grant(
    grant: &RecordingEventsReadGrant,
    reader: &mut dyn AgentAttachmentReader,
) -> Result<Vec<u8>, BridgeForwardError> {
    let message = reader.read_events(grant)?;
    match &message {
        WireMessage::AppRecordingEventsChunk(value)
            if value.protocol_version == grant.protocol_version
                && value.controller_lease_id == grant.controller_lease_id
                && value.recording_id == grant.recording_id
                && value.event_stream_id == grant.event_stream_id
                && value.request_id == grant.request_id
                && value.sequence == grant.sequence
                && value.chunk_index == grant.chunk_index
                && value.block_bytes == grant.block_bytes
                && value.block_sha256 == grant.block_sha256 => {}
        _ => return Err(BridgeForwardError::AttachmentRead),
    }
    serde_json::to_vec(&message).map_err(|_| BridgeForwardError::AttachmentRead)
}
