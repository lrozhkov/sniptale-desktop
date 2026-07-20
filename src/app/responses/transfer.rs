//! Native-host transfer acknowledgement policy.

mod chunks;

use serde_json::Value;

use crate::protocol::{OperationFailedOperation, OperationalErrorKind, WireMessage};

use self::chunks::{recording_chunk_response, screenshot_chunk_response};
use super::{
    NativeHostSideEffects,
    outbound::{OperationFailure, operation_failed},
    session::ActiveLeaseValidation,
};

#[derive(Clone, Copy)]
pub(super) struct TransferInputs<'a> {
    pub(super) controller_lease_id: &'a str,
    pub(super) operation: OperationFailedOperation,
    pub(super) operation_id: &'a str,
    pub(super) action: TransferAction,
}

#[derive(Clone, Copy)]
pub(super) enum TransferAction {
    ScreenshotChunkRequest { chunk_index: u32 },
    ScreenshotAck,
    ScreenshotReject { retain_for_diagnostics: bool },
    RecordingChunkRequest { chunk_index: u32 },
    RecordingAck,
    RecordingReject { retain_for_diagnostics: bool },
}

impl<'a> TransferInputs<'a> {
    pub(super) fn from_message(message: &'a WireMessage) -> Option<Self> {
        match message {
            WireMessage::ExtensionScreenshotAck(ack) => Some(Self {
                controller_lease_id: &ack.controller_lease_id,
                operation: OperationFailedOperation::Screenshot,
                operation_id: &ack.capture_id,
                action: TransferAction::ScreenshotAck,
            }),
            WireMessage::ExtensionScreenshotReject(reject) => Some(Self {
                controller_lease_id: &reject.controller_lease_id,
                operation: OperationFailedOperation::Screenshot,
                operation_id: &reject.capture_id,
                action: TransferAction::ScreenshotReject {
                    retain_for_diagnostics: reject.diagnostic_retention_requested.unwrap_or(false),
                },
            }),
            WireMessage::ExtensionScreenshotChunkRequest(request) => Some(Self {
                controller_lease_id: &request.controller_lease_id,
                operation: OperationFailedOperation::Screenshot,
                operation_id: &request.capture_id,
                action: TransferAction::ScreenshotChunkRequest {
                    chunk_index: request.chunk_index,
                },
            }),
            WireMessage::ExtensionRecordingChunkRequest(request) => Some(Self {
                controller_lease_id: &request.controller_lease_id,
                operation: OperationFailedOperation::Recording,
                operation_id: &request.recording_id,
                action: TransferAction::RecordingChunkRequest {
                    chunk_index: request.chunk_index,
                },
            }),
            WireMessage::ExtensionRecordingAck(ack) => Some(Self {
                controller_lease_id: &ack.controller_lease_id,
                operation: OperationFailedOperation::Recording,
                operation_id: &ack.recording_id,
                action: TransferAction::RecordingAck,
            }),
            WireMessage::ExtensionRecordingReject(reject) => Some(Self {
                controller_lease_id: &reject.controller_lease_id,
                operation: OperationFailedOperation::Recording,
                operation_id: &reject.recording_id,
                action: TransferAction::RecordingReject {
                    retain_for_diagnostics: reject.diagnostic_retention_requested.unwrap_or(false),
                },
            }),
            _ => None,
        }
    }
}

pub(super) fn transfer_response(
    lease_validation: ActiveLeaseValidation,
    observed_epoch_ms: u64,
    inputs: TransferInputs<'_>,
    effects: &mut impl NativeHostSideEffects,
) -> Vec<Value> {
    match lease_validation {
        ActiveLeaseValidation::Valid => {}
        ActiveLeaseValidation::Invalid => {
            return vec![stale_lease_failure(observed_epoch_ms, &inputs)];
        }
        ActiveLeaseValidation::StateUnavailable => {
            return vec![lease_state_unavailable_failure(observed_epoch_ms, &inputs)];
        }
    }
    match inputs.action {
        TransferAction::ScreenshotChunkRequest { chunk_index } => {
            screenshot_chunk_response(observed_epoch_ms, inputs, chunk_index, effects)
        }
        TransferAction::ScreenshotAck => {
            screenshot_ack_response(observed_epoch_ms, inputs, effects)
        }
        TransferAction::ScreenshotReject {
            retain_for_diagnostics,
        } => screenshot_reject_response(observed_epoch_ms, inputs, retain_for_diagnostics, effects),
        TransferAction::RecordingChunkRequest { chunk_index } => {
            recording_chunk_response(observed_epoch_ms, inputs, chunk_index, effects)
        }
        TransferAction::RecordingAck => recording_ack_response(observed_epoch_ms, inputs, effects),
        TransferAction::RecordingReject {
            retain_for_diagnostics,
        } => recording_reject_response(observed_epoch_ms, inputs, retain_for_diagnostics, effects),
    }
}

fn stale_lease_failure(observed_epoch_ms: u64, inputs: &TransferInputs<'_>) -> Value {
    transfer_failure(
        observed_epoch_ms,
        inputs,
        "lease-validation",
        OperationalErrorKind::StaleControllerLease,
        None,
    )
}

fn lease_state_unavailable_failure(observed_epoch_ms: u64, inputs: &TransferInputs<'_>) -> Value {
    transfer_failure(
        observed_epoch_ms,
        inputs,
        "lease-validation",
        OperationalErrorKind::StorageUnavailable,
        None,
    )
}

fn no_pending_transfer_failure(observed_epoch_ms: u64, inputs: &TransferInputs<'_>) -> Value {
    transfer_failure(
        observed_epoch_ms,
        inputs,
        "transfer-state",
        OperationalErrorKind::TransferNotPending,
        None,
    )
}

fn transfer_state_storage_failure(observed_epoch_ms: u64, inputs: &TransferInputs<'_>) -> Value {
    transfer_failure(
        observed_epoch_ms,
        inputs,
        "transfer-state",
        OperationalErrorKind::StorageUnavailable,
        None,
    )
}

fn transfer_failure(
    observed_epoch_ms: u64,
    inputs: &TransferInputs<'_>,
    phase: &str,
    kind: OperationalErrorKind,
    message: Option<&str>,
) -> Value {
    operation_failed(OperationFailure {
        controller_lease_id: Some(inputs.controller_lease_id),
        operation: inputs.operation,
        operation_id: Some(inputs.operation_id),
        phase,
        kind,
        message,
        occurred_at_epoch_ms: observed_epoch_ms,
    })
}

fn screenshot_ack_response(
    observed_epoch_ms: u64,
    inputs: TransferInputs<'_>,
    effects: &mut impl NativeHostSideEffects,
) -> Vec<Value> {
    match effects.ack_screenshot_transfer(inputs.operation_id) {
        Ok(true) => Vec::new(),
        Ok(false) => vec![no_pending_transfer_failure(observed_epoch_ms, &inputs)],
        Err(_) => vec![transfer_state_storage_failure(observed_epoch_ms, &inputs)],
    }
}

fn screenshot_reject_response(
    observed_epoch_ms: u64,
    inputs: TransferInputs<'_>,
    retain_for_diagnostics: bool,
    effects: &mut impl NativeHostSideEffects,
) -> Vec<Value> {
    match effects.reject_screenshot_transfer(inputs.operation_id, retain_for_diagnostics) {
        Ok(true) => Vec::new(),
        Ok(false) => vec![no_pending_transfer_failure(observed_epoch_ms, &inputs)],
        Err(_) => vec![transfer_state_storage_failure(observed_epoch_ms, &inputs)],
    }
}

fn recording_ack_response(
    observed_epoch_ms: u64,
    inputs: TransferInputs<'_>,
    effects: &mut impl NativeHostSideEffects,
) -> Vec<Value> {
    match effects.ack_recording_transfer(inputs.operation_id) {
        Ok(true) => Vec::new(),
        Ok(false) => vec![no_pending_transfer_failure(observed_epoch_ms, &inputs)],
        Err(_) => vec![transfer_state_storage_failure(observed_epoch_ms, &inputs)],
    }
}

fn recording_reject_response(
    observed_epoch_ms: u64,
    inputs: TransferInputs<'_>,
    retain_for_diagnostics: bool,
    effects: &mut impl NativeHostSideEffects,
) -> Vec<Value> {
    match effects.reject_recording_transfer(inputs.operation_id, retain_for_diagnostics) {
        Ok(true) => Vec::new(),
        Ok(false) => vec![no_pending_transfer_failure(observed_epoch_ms, &inputs)],
        Err(_) => vec![transfer_state_storage_failure(observed_epoch_ms, &inputs)],
    }
}
