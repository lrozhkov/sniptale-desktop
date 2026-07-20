//! Reconnect notifications for finalized media recovered by the resident agent.

use serde_json::Value;

#[cfg(any(windows, test))]
use crate::capture::PendingScreenshotTransfer;
#[cfg(any(windows, test))]
use crate::protocol::{AppScreenshotStartMessage, ScreenshotMimeType};
use crate::{
    protocol::{
        AppRecordingStoppedMessage, OperationFailedOperation, OperationalErrorKind,
        PROTOCOL_VERSION, RecordingMimeType,
    },
    recording::PendingRecordingTransfer,
};

#[cfg(any(windows, test))]
use super::{NativeHostCommandEvent, outbound::command_event};
use super::{
    NativeHostSideEffectError, NativeHostSideEffects,
    outbound::{OperationFailure, operation_failed, recording_stopped},
};

#[cfg(any(windows, test))]
pub(super) fn append_pending_screenshot_notifications(
    responses: &mut Vec<Value>,
    controller_lease_id: &str,
    observed_epoch_ms: u64,
    effects: &mut impl NativeHostSideEffects,
) {
    match effects.pending_screenshots_for_reconnect() {
        Ok(pending) => {
            for screenshot in pending {
                responses.push(screenshot_start_message(controller_lease_id, screenshot));
            }
        }
        Err(_) => responses.push(screenshot_recovery_failed(
            controller_lease_id,
            observed_epoch_ms,
        )),
    }
}

#[cfg(not(any(windows, test)))]
pub(super) fn append_pending_screenshot_notifications(
    _responses: &mut Vec<Value>,
    _controller_lease_id: &str,
    _observed_epoch_ms: u64,
    _effects: &mut impl NativeHostSideEffects,
) {
}

pub(super) fn append_pending_recording_notifications(
    responses: &mut Vec<Value>,
    controller_lease_id: &str,
    observed_epoch_ms: u64,
    effects: &mut impl NativeHostSideEffects,
) {
    match effects.pending_recordings_for_reconnect() {
        Ok(pending) => {
            for recording in pending {
                match recording_stopped_message(controller_lease_id, recording) {
                    Ok(message) => responses.push(message),
                    Err(_) => responses.push(recording_recovery_failed(
                        controller_lease_id,
                        observed_epoch_ms,
                    )),
                }
            }
        }
        Err(_) => responses.push(recording_recovery_failed(
            controller_lease_id,
            observed_epoch_ms,
        )),
    }
}

#[cfg(any(windows, test))]
fn screenshot_start_message(
    controller_lease_id: &str,
    screenshot: PendingScreenshotTransfer,
) -> Value {
    command_event(NativeHostCommandEvent::ScreenshotStart(
        AppScreenshotStartMessage {
            protocol_version: PROTOCOL_VERSION,
            controller_lease_id: controller_lease_id.to_owned(),
            capture_id: screenshot.capture_id,
            mode: screenshot.mode,
            filename: screenshot.filename,
            mime_type: ScreenshotMimeType::ImagePng,
            total_bytes: screenshot.total_bytes,
            width: screenshot.width,
            height: screenshot.height,
            chunk_count: screenshot.chunk_count,
            sha256: screenshot.sha256,
            open_editor: screenshot.open_editor,
            captured_at_epoch_ms: screenshot.captured_at_epoch_ms,
        },
    ))
}

fn recording_stopped_message(
    controller_lease_id: &str,
    recording: PendingRecordingTransfer,
) -> Result<Value, NativeHostSideEffectError> {
    let chunk_count = u32::try_from(recording.chunk_count)
        .map_err(|_| NativeHostSideEffectError::RecordingRecovery)?;
    Ok(recording_stopped(AppRecordingStoppedMessage {
        protocol_version: PROTOCOL_VERSION,
        controller_lease_id: controller_lease_id.to_owned(),
        recording_id: recording.recording_id,
        duration_ms: recording.duration_ms,
        filename: recording.filename,
        mime_type: RecordingMimeType::VideoMp4,
        total_bytes: recording.total_bytes,
        width: recording.width,
        height: recording.height,
        fps: recording.fps,
        chunk_count,
        sha256: recording.sha256,
        telemetry: None,
        open_editor: recording.open_editor,
    }))
}

#[cfg(any(windows, test))]
fn screenshot_recovery_failed(controller_lease_id: &str, observed_epoch_ms: u64) -> Value {
    operation_failed(OperationFailure {
        controller_lease_id: Some(controller_lease_id),
        operation: OperationFailedOperation::Screenshot,
        operation_id: Some("screenshot-reconnect"),
        phase: "startup-recovery",
        kind: OperationalErrorKind::StorageUnavailable,
        message: None,
        occurred_at_epoch_ms: observed_epoch_ms,
    })
}

fn recording_recovery_failed(controller_lease_id: &str, observed_epoch_ms: u64) -> Value {
    operation_failed(OperationFailure {
        controller_lease_id: Some(controller_lease_id),
        operation: OperationFailedOperation::Recording,
        operation_id: Some("recording-reconnect"),
        phase: "startup-recovery",
        kind: OperationalErrorKind::RecordingRecoveryUnavailable,
        message: None,
        occurred_at_epoch_ms: observed_epoch_ms,
    })
}
