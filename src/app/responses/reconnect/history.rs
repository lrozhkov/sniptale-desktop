//! Bounded per-lease discovery for history streams whose video was already acknowledged.

use crate::app::responses::{NativeHostSession, NativeHostSideEffects, outbound};
use crate::protocol::{
    OperationFailedOperation, OperationalErrorKind, PROTOCOL_VERSION,
    recording_events::AppRecordingEventsAvailableMessage,
    trajectory::AppRecordingAttachmentAvailableMessage,
};
use serde_json::Value;
use std::collections::BTreeSet;

pub(in crate::app::responses) fn append_history_notifications(
    session: &mut NativeHostSession,
    effects: &mut impl NativeHostSideEffects,
    responses: &mut Vec<Value>,
    pending: &mut BTreeSet<String>,
    lease: &str,
) {
    if session.recording_formats.cursor_trajectory {
        cursors(session, effects, responses, pending, lease);
    }
    if session.recording_formats.recording_events {
        events(session, effects, responses, pending, lease);
    }
}

fn cursors(
    session: &mut NativeHostSession,
    effects: &mut impl NativeHostSideEffects,
    responses: &mut Vec<Value>,
    pending: &mut BTreeSet<String>,
    lease: &str,
) {
    let Ok(items) = effects.pending_recording_attachments() else {
        responses.push(failure(
            lease,
            "cursor-attachment-recovery",
            session.current_epoch_ms(),
        ));
        return;
    };
    for item in items.into_iter().take(16) {
        let Some(video) = item.video else { continue };
        let key = format!(
            "cursor:{}:{}",
            item.recording_id, item.descriptor.trajectory_id
        );
        pending.insert(key.clone());
        if session.announced_media.insert(key) {
            responses.push(outbound::cursor_attachment_available(
                AppRecordingAttachmentAvailableMessage {
                    protocol_version: PROTOCOL_VERSION,
                    controller_lease_id: lease.to_owned(),
                    recording_id: item.recording_id,
                    trajectory_id: item.descriptor.trajectory_id.clone(),
                    video_sha256: video.sha256,
                    descriptor: item.descriptor,
                },
            ));
        }
    }
}

fn events(
    session: &mut NativeHostSession,
    effects: &mut impl NativeHostSideEffects,
    responses: &mut Vec<Value>,
    pending: &mut BTreeSet<String>,
    lease: &str,
) {
    let Ok(items) = effects.pending_recording_event_attachments() else {
        responses.push(failure(
            lease,
            "event-attachment-recovery",
            session.current_epoch_ms(),
        ));
        return;
    };
    for item in items.into_iter().take(16) {
        let Some(video) = item.video else { continue };
        let key = format!(
            "events:{}:{}",
            item.recording_id, item.descriptor.event_stream_id
        );
        pending.insert(key.clone());
        if session.announced_media.insert(key) {
            responses.push(outbound::events_attachment_available(
                AppRecordingEventsAvailableMessage {
                    protocol_version: PROTOCOL_VERSION,
                    controller_lease_id: lease.to_owned(),
                    recording_id: item.recording_id,
                    event_stream_id: item.descriptor.event_stream_id.clone(),
                    video_sha256: video.sha256,
                    descriptor: item.descriptor,
                },
            ));
        }
    }
}

fn failure(lease: &str, phase: &'static str, now: u64) -> Value {
    outbound::operation_failed(outbound::OperationFailure {
        controller_lease_id: Some(lease),
        operation: OperationFailedOperation::Recording,
        operation_id: None,
        phase,
        kind: OperationalErrorKind::StorageUnavailable,
        message: None,
        occurred_at_epoch_ms: now,
    })
}
