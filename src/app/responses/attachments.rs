//! Current-lease attachment authorization; internal read grants contain no payload bytes.

use super::{
    NativeHostSession, NativeHostSideEffects,
    outbound::{OperationFailure, operation_failed},
    session::ActiveLeaseValidation,
};
use crate::protocol::{
    OperationFailedOperation, OperationalErrorKind, WireMessage,
    trajectory::trajectory_read_grant_value,
};
use serde_json::Value;

pub(super) fn responses(
    session: &NativeHostSession,
    message: &WireMessage,
    effects: &mut impl NativeHostSideEffects,
) -> Option<Vec<Value>> {
    let (lease, recording) = match message {
        WireMessage::ExtensionRecordingAttachmentManifestRequest(value) => {
            (&value.controller_lease_id, &value.recording_id)
        }
        WireMessage::ExtensionRecordingAttachmentChunkRequest(value) => {
            (&value.controller_lease_id, &value.recording_id)
        }
        WireMessage::ExtensionRecordingAttachmentAck(value) => {
            (&value.controller_lease_id, &value.recording_id)
        }
        _ => return None,
    };
    if !session.handshake_accepted || !session.recording_formats.cursor_trajectory {
        return Some(vec![failure(
            session,
            lease,
            recording,
            OperationalErrorKind::UnsupportedCapability,
        )]);
    }
    match session.validate_active_lease(lease, session.current_epoch_ms()) {
        ActiveLeaseValidation::Valid => {}
        ActiveLeaseValidation::Invalid => {
            return Some(vec![failure(
                session,
                lease,
                recording,
                OperationalErrorKind::StaleControllerLease,
            )]);
        }
        ActiveLeaseValidation::StateUnavailable => {
            return Some(vec![failure(
                session,
                lease,
                recording,
                OperationalErrorKind::StorageUnavailable,
            )]);
        }
    }
    let result = match message {
        WireMessage::ExtensionRecordingAttachmentManifestRequest(request) => effects
            .recording_attachment_manifest(request)
            .and_then(|response| {
                serde_json::to_value(WireMessage::AppRecordingAttachmentManifest(Box::new(
                    response,
                )))
                .map_err(|_| super::NativeHostSideEffectError::RecordingTransferState)
            })
            .map(|value| vec![value]),
        WireMessage::ExtensionRecordingAttachmentChunkRequest(request) => effects
            .recording_attachment_read_grant(request)
            .and_then(|grant| {
                trajectory_read_grant_value(grant)
                    .map_err(|_| super::NativeHostSideEffectError::RecordingTransferState)
            })
            .map(|value| vec![value]),
        WireMessage::ExtensionRecordingAttachmentAck(ack) => {
            match effects.ack_recording_attachment(ack) {
                Ok(true) => Ok(Vec::new()),
                Ok(false) => {
                    return Some(vec![failure(
                        session,
                        lease,
                        recording,
                        OperationalErrorKind::TransferNotPending,
                    )]);
                }
                Err(error) => Err(error),
            }
        }
        _ => return None,
    };
    Some(result.unwrap_or_else(|_| {
        vec![failure(
            session,
            lease,
            recording,
            OperationalErrorKind::StorageUnavailable,
        )]
    }))
}

fn failure(
    session: &NativeHostSession,
    lease: &str,
    recording: &str,
    kind: OperationalErrorKind,
) -> Value {
    operation_failed(OperationFailure {
        controller_lease_id: Some(lease),
        operation: OperationFailedOperation::Recording,
        operation_id: Some(recording),
        phase: "cursor-attachment",
        kind,
        message: None,
        occurred_at_epoch_ms: session.current_epoch_ms(),
    })
}
