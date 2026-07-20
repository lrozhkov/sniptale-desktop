//! Resident media worker event projection.

use crate::{
    app::responses::NativeHostCommandEvent,
    protocol::{
        AppRecordingProgressMessage, AppRecordingStartedMessage, AppRecordingStoppedMessage,
        PROTOCOL_VERSION, RecordingMimeType,
    },
    recording::{RecordingCommandEvent, RecordingStoppedEvent},
};

pub(super) fn recording_command_event(
    event: RecordingCommandEvent,
    controller_lease_id: &str,
) -> Option<NativeHostCommandEvent> {
    Some(match event {
        RecordingCommandEvent::Started(event) => {
            NativeHostCommandEvent::RecordingStarted(AppRecordingStartedMessage {
                protocol_version: PROTOCOL_VERSION,
                controller_lease_id: controller_lease_id.to_owned(),
                recording_id: event.recording_id,
                source: event.source,
                timebase: event.timebase,
                requested_settings_revision: event.requested_settings_revision,
                requested_quality: event.requested_quality,
                effective_quality: event.effective_quality,
                started_at_epoch_ms: event.started_at_epoch_ms,
            })
        }
        RecordingCommandEvent::Progress(event) => {
            NativeHostCommandEvent::RecordingProgress(AppRecordingProgressMessage {
                protocol_version: PROTOCOL_VERSION,
                controller_lease_id: controller_lease_id.to_owned(),
                recording_id: event.recording_id,
                duration_ms: event.duration_ms,
                bytes_written: event.bytes_written,
                status: event.status,
            })
        }
        RecordingCommandEvent::Stopped(event) => {
            NativeHostCommandEvent::RecordingStopped(stopped_message(event, controller_lease_id))
        }
        RecordingCommandEvent::ManualFinalized(_) => return None,
    })
}

fn stopped_message(
    event: RecordingStoppedEvent,
    controller_lease_id: &str,
) -> AppRecordingStoppedMessage {
    let pending = event.pending;
    AppRecordingStoppedMessage {
        protocol_version: PROTOCOL_VERSION,
        controller_lease_id: controller_lease_id.to_owned(),
        recording_id: pending.recording_id,
        duration_ms: pending.duration_ms,
        filename: pending.filename,
        mime_type: RecordingMimeType::VideoMp4,
        total_bytes: pending.total_bytes,
        width: pending.width,
        height: pending.height,
        fps: pending.fps,
        chunk_count: u32::try_from(pending.chunk_count).unwrap_or(u32::MAX),
        sha256: pending.sha256,
        telemetry: event.telemetry,
        open_editor: pending.open_editor,
    }
}
