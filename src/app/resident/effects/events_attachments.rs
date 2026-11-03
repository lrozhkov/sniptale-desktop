//! Selected-browser authorization and typed attachment orchestration; bytes stay transfer-owned.

use super::agent::AgentSideEffects;
use crate::{
    app::responses::NativeHostSideEffectError,
    protocol::recording_events::{
        AppRecordingEventsManifestMessage, ExtensionRecordingEventsAckMessage,
        ExtensionRecordingEventsChunkRequestMessage,
        ExtensionRecordingEventsManifestRequestMessage, RecordingEventsReadGrant,
    },
    transfer::{events_catalog::PendingEvents, events_store::EventsArchive},
};

impl AgentSideEffects {
    fn events_archive(
        &self,
        recording: &str,
        trajectory: &str,
        mutation: bool,
    ) -> Result<EventsArchive, NativeHostSideEffectError> {
        let target = self
            .browser_target_id
            .as_deref()
            .ok_or(NativeHostSideEffectError::RecordingTransferState)?;
        let catalog = self
            .recordings
            .as_ref()
            .ok_or(NativeHostSideEffectError::RecordingTransferState)?
            .recording_events_catalog();
        let assignments = self.assignment_store()?;
        let archive = if mutation {
            catalog.authorized_mutation(recording, trajectory, target, &assignments)
        } else {
            catalog.authorized_read(recording, trajectory, target, &assignments)
        };
        archive.map_err(|_| NativeHostSideEffectError::RecordingTransferState)
    }

    pub(super) fn recording_events_manifest_from_store(
        &self,
        request: &ExtensionRecordingEventsManifestRequestMessage,
    ) -> Result<AppRecordingEventsManifestMessage, NativeHostSideEffectError> {
        let mut archive =
            self.events_archive(&request.recording_id, &request.event_stream_id, false)?;
        let entries = archive
            .manifest(request.next_sequence, &request.prefix_sha256)
            .map_err(|_| NativeHostSideEffectError::RecordingTransferState)?;
        let end = request.next_sequence
            + u64::try_from(entries.len())
                .map_err(|_| NativeHostSideEffectError::RecordingTransferState)?;
        let descriptor = archive.descriptor().clone();
        Ok(AppRecordingEventsManifestMessage {
            protocol_version: request.protocol_version,
            controller_lease_id: request.controller_lease_id.clone(),
            recording_id: request.recording_id.clone(),
            event_stream_id: request.event_stream_id.clone(),
            request_id: request.request_id.clone(),
            from_sequence: request.next_sequence,
            next_sequence: (end < descriptor.total_blocks).then_some(end),
            descriptor,
            entries,
        })
    }

    pub(super) fn recording_events_grant_from_store(
        &self,
        request: &ExtensionRecordingEventsChunkRequestMessage,
    ) -> Result<RecordingEventsReadGrant, NativeHostSideEffectError> {
        let mut archive =
            self.events_archive(&request.recording_id, &request.event_stream_id, false)?;
        let entry = archive
            .block_entry(request.sequence)
            .map_err(|_| NativeHostSideEffectError::RecordingTransferState)?;
        if u32::from(request.chunk_index) >= entry.byte_length.div_ceil(32768) {
            return Err(NativeHostSideEffectError::RecordingTransferState);
        }
        Ok(RecordingEventsReadGrant {
            protocol_version: request.protocol_version,
            controller_lease_id: request.controller_lease_id.clone(),
            recording_id: request.recording_id.clone(),
            event_stream_id: request.event_stream_id.clone(),
            request_id: request.request_id.clone(),
            sequence: request.sequence,
            chunk_index: request.chunk_index,
            block_bytes: entry.byte_length,
            block_sha256: entry.block_sha256,
        })
    }

    pub(super) fn ack_recording_events_in_store(
        &self,
        ack: &ExtensionRecordingEventsAckMessage,
    ) -> Result<bool, NativeHostSideEffectError> {
        if !self.recording_receipt_assignment_exists(&ack.recording_id)? {
            return Ok(false);
        }
        let mut archive = self.events_archive(&ack.recording_id, &ack.event_stream_id, true)?;
        archive
            .accept_event_receipt(&ack.receipt)
            .map_err(|_| NativeHostSideEffectError::RecordingTransferState)?;
        drop(archive);
        self.cleanup_recording_history_assignment(&ack.recording_id)?;
        Ok(true)
    }

    pub(super) fn pending_recording_event_attachments_from_store(
        &self,
    ) -> Result<Vec<PendingEvents>, NativeHostSideEffectError> {
        let Some(target) = self.browser_target_id.as_deref() else {
            return Ok(Vec::new());
        };
        let catalog = self
            .recordings
            .as_ref()
            .ok_or(NativeHostSideEffectError::RecordingTransferState)?
            .recording_events_catalog();
        let assignments = self.assignment_store()?;
        catalog
            .pending_after_video_ack(target, &assignments)
            .map_err(|_| NativeHostSideEffectError::RecordingTransferState)
    }
}
