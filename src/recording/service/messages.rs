//! Recording protocol message builders.

use super::super::{
    PendingRecordingTransfer, runtime::FinalizedRecordingArtifact, session::RecordingState,
    temp_media::RecordingMediaMetadata,
};
use crate::protocol::{
    NativeEffectiveQualitySettings, NativeRecordingTimebase, NativeRecordingTimebaseUnits,
    RecordingProgressStatus,
};
use crate::recording::runtime::RecordingRuntimeError;
use crate::recording::{
    RecordingProgressEvent, RecordingStartRequest, RecordingStartedEvent, RecordingStoppedEvent,
};
use crate::transfer::identity::{TransferMediaFileKind, media_filename};

pub(super) fn recording_started_event(
    request: &RecordingStartRequest,
    recording_id: String,
    effective_quality: NativeEffectiveQualitySettings,
    started_at_monotonic_ns: String,
) -> RecordingStartedEvent {
    let timebase_id = format!("timebase-{recording_id}");
    RecordingStartedEvent {
        recording_id,
        source: request.source.clone(),
        timebase: NativeRecordingTimebase {
            id: timebase_id,
            started_at_epoch_ms: request.requested_at_epoch_ms,
            started_at_monotonic_ns,
            units: NativeRecordingTimebaseUnits::Milliseconds,
        },
        requested_settings_revision: request.settings_revision.clone(),
        requested_quality: request.requested_quality.clone(),
        effective_quality,
        started_at_epoch_ms: request.requested_at_epoch_ms,
    }
}

pub(super) fn recording_progress_event(
    recording_id: &str,
    duration_ms: u64,
    status: RecordingProgressStatus,
) -> RecordingProgressEvent {
    RecordingProgressEvent {
        recording_id: recording_id.to_owned(),
        duration_ms,
        bytes_written: 0,
        status,
    }
}

pub(super) fn recording_metadata(
    recording_id: &str,
    artifact: FinalizedRecordingArtifact,
    open_editor: bool,
) -> Result<RecordingMediaMetadata, RecordingRuntimeError> {
    let filename = media_filename(recording_id, TransferMediaFileKind::Mp4)
        .map_err(|_| RecordingRuntimeError::TransferState)?;
    Ok(RecordingMediaMetadata {
        filename,
        duration_ms: artifact.duration_ms,
        width: artifact.width,
        height: artifact.height,
        fps: artifact.fps,
        open_editor,
    })
}

pub(super) fn recording_stopped_event(
    pending: PendingRecordingTransfer,
    telemetry: Option<crate::protocol::NativeRecordingTelemetrySnapshot>,
) -> RecordingStoppedEvent {
    RecordingStoppedEvent { pending, telemetry }
}

pub(super) const fn progress_status(state: RecordingState) -> Option<RecordingProgressStatus> {
    match state {
        RecordingState::Recording => Some(RecordingProgressStatus::Recording),
        RecordingState::Paused => Some(RecordingProgressStatus::Paused),
        RecordingState::Stopping => Some(RecordingProgressStatus::Stopping),
        RecordingState::Finalizing => Some(RecordingProgressStatus::Finalizing),
        _ => None,
    }
}
