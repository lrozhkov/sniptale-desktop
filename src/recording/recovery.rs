//! Reconnect reporting projection for recovered recordings.

#[cfg(any(windows, test))]
use super::temp_media::{RecoveredTempMedia, StartupRecoveryDecision};
#[cfg(any(windows, test))]
use crate::transfer::{chunks::chunk_count, error::TransferError};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PendingRecordingTransfer {
    pub(crate) recording_events: Option<crate::protocol::recording_events::EventDescriptor>,
    pub(crate) cursor_trajectory: Option<crate::protocol::trajectory::CursorTrajectoryDescriptor>,
    #[cfg(any(windows, test))]
    pub(crate) telemetry: Option<crate::protocol::NativeRecordingTelemetrySnapshot>,
    #[cfg(any(windows, test))]
    pub(crate) delivery_route: Option<crate::protocol::DeliveryRoute>,
    pub(crate) recording_id: String,
    pub(crate) filename: String,
    pub(crate) duration_ms: u64,
    pub(crate) total_bytes: u64,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) fps: u16,
    pub(crate) sha256: String,
    pub(crate) chunk_count: u64,
    pub(crate) next_missing_chunk_index: u64,
    pub(crate) open_editor: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(any(windows, test))]
pub(crate) enum RecordingRecoveryError {
    MissingFinalizedMetadata,
    Transfer(TransferError),
}

#[cfg(any(windows, test))]
pub(crate) fn pending_recordings_for_reconnect(
    recovered: &[RecoveredTempMedia],
) -> Result<Vec<PendingRecordingTransfer>, RecordingRecoveryError> {
    let mut pending = Vec::new();
    for item in recovered {
        if item.decision != StartupRecoveryDecision::ReportPending {
            continue;
        }
        if !matches!(
            (&item.record.authority, &item.record.delivery_route),
            (None, None)
                | (
                    Some(crate::protocol::OperationAuthority::ExtensionManaged { .. }),
                    Some(crate::protocol::DeliveryRoute::ExtensionTransfer),
                )
                | (
                    Some(crate::protocol::OperationAuthority::Manual { .. }),
                    Some(crate::protocol::DeliveryRoute::DesktopDelivery { .. })
                )
        ) {
            continue;
        }
        let total_bytes = item
            .record
            .total_bytes
            .ok_or(RecordingRecoveryError::MissingFinalizedMetadata)?;
        let sha256 = item
            .record
            .sha256
            .clone()
            .ok_or(RecordingRecoveryError::MissingFinalizedMetadata)?;
        let recording = item
            .record
            .recording
            .clone()
            .ok_or(RecordingRecoveryError::MissingFinalizedMetadata)?;
        pending.push(PendingRecordingTransfer {
            recording_events: recording.recording_events.clone(),
            cursor_trajectory: recording.cursor_trajectory.clone(),
            telemetry: item.record.telemetry.as_ref().and_then(|snapshot| {
                crate::telemetry::browser_delivery_telemetry(
                    snapshot,
                    recording.width,
                    recording.height,
                    recording.duration_ms,
                )
            }),
            delivery_route: item.record.delivery_route.clone(),
            recording_id: item.record.media_id.clone(),
            filename: recording.filename,
            duration_ms: recording.duration_ms,
            total_bytes,
            width: recording.width,
            height: recording.height,
            fps: recording.fps,
            sha256,
            chunk_count: chunk_count(total_bytes).map_err(RecordingRecoveryError::Transfer)?,
            next_missing_chunk_index: item.record.next_missing_chunk_index,
            open_editor: recording.open_editor,
        });
    }
    pending.sort_by(|left, right| left.recording_id.cmp(&right.recording_id));
    Ok(pending)
}
