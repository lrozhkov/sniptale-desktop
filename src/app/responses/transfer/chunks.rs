//! Native-host transfer chunk response policy.

use serde_json::Value;

#[cfg(not(any(windows, test)))]
use crate::app::responses::NativeHostSideEffects;
use crate::protocol::OperationalErrorKind;
#[cfg(any(windows, test))]
use crate::{
    app::responses::{NativeHostSideEffects, NativeHostTransferChunkError},
    transfer::messages::{recording_chunk_value, screenshot_chunk_value, screenshot_commit_value},
};

use super::TransferInputs;
use super::transfer_failure;

pub(super) fn screenshot_chunk_response(
    observed_epoch_ms: u64,
    inputs: TransferInputs<'_>,
    chunk_index: u32,
    effects: &mut impl NativeHostSideEffects,
) -> Vec<Value> {
    #[cfg(not(any(windows, test)))]
    {
        let _ = (chunk_index, effects);
        native_transport_unavailable_failure(observed_epoch_ms, &inputs)
    }
    #[cfg(any(windows, test))]
    {
        match effects.screenshot_transfer_chunk(inputs.operation_id, chunk_index, observed_epoch_ms)
        {
            Ok(Some(transfer)) => {
                let chunk_value = screenshot_chunk_value(
                    inputs.controller_lease_id,
                    inputs.operation_id,
                    transfer.chunk,
                );
                let commit_value = transfer.is_complete.then(|| {
                    screenshot_commit_value(inputs.controller_lease_id, inputs.operation_id)
                });
                screenshot_chunk_values(observed_epoch_ms, &inputs, chunk_value, commit_value)
            }
            Ok(None) => transfer_not_pending_failure(observed_epoch_ms, &inputs),
            Err(error) => chunk_error_failure(observed_epoch_ms, &inputs, error),
        }
    }
}

#[cfg(any(windows, test))]
fn screenshot_chunk_values(
    observed_epoch_ms: u64,
    inputs: &TransferInputs<'_>,
    chunk_value: Result<Value, crate::transfer::messages::TransferChunkMessageError>,
    commit_value: Option<Result<Value, crate::transfer::messages::TransferChunkMessageError>>,
) -> Vec<Value> {
    let Ok(chunk_value) = chunk_value else {
        return serialization_failure(observed_epoch_ms, inputs);
    };
    let Some(commit_value) = commit_value else {
        return vec![chunk_value];
    };
    let Ok(commit_value) = commit_value else {
        return serialization_failure(observed_epoch_ms, inputs);
    };
    vec![chunk_value, commit_value]
}

pub(super) fn recording_chunk_response(
    observed_epoch_ms: u64,
    inputs: TransferInputs<'_>,
    chunk_index: u32,
    effects: &mut impl NativeHostSideEffects,
) -> Vec<Value> {
    #[cfg(not(any(windows, test)))]
    {
        let _ = (chunk_index, effects);
        native_transport_unavailable_failure(observed_epoch_ms, &inputs)
    }
    #[cfg(any(windows, test))]
    {
        match effects.recording_transfer_chunk(inputs.operation_id, chunk_index, observed_epoch_ms)
        {
            Ok(Some(chunk)) => {
                match recording_chunk_value(inputs.controller_lease_id, inputs.operation_id, chunk)
                {
                    Ok(value) => vec![value],
                    Err(_) => serialization_failure(observed_epoch_ms, &inputs),
                }
            }
            Ok(None) => transfer_not_pending_failure(observed_epoch_ms, &inputs),
            Err(error) => chunk_error_failure(observed_epoch_ms, &inputs, error),
        }
    }
}

#[cfg(not(any(windows, test)))]
fn native_transport_unavailable_failure(
    observed_epoch_ms: u64,
    inputs: &TransferInputs<'_>,
) -> Vec<Value> {
    chunk_failure(
        observed_epoch_ms,
        inputs,
        "transfer-channel",
        OperationalErrorKind::UnsupportedCapability,
    )
}

#[cfg(any(windows, test))]
fn transfer_not_pending_failure(observed_epoch_ms: u64, inputs: &TransferInputs<'_>) -> Vec<Value> {
    chunk_failure(
        observed_epoch_ms,
        inputs,
        "transfer-state",
        OperationalErrorKind::TransferNotPending,
    )
}

#[cfg(any(windows, test))]
fn chunk_error_failure(
    observed_epoch_ms: u64,
    inputs: &TransferInputs<'_>,
    error: NativeHostTransferChunkError,
) -> Vec<Value> {
    match error {
        NativeHostTransferChunkError::State => chunk_failure(
            observed_epoch_ms,
            inputs,
            "transfer-state",
            OperationalErrorKind::StorageUnavailable,
        ),
        NativeHostTransferChunkError::ChunkOutOfBounds => chunk_failure(
            observed_epoch_ms,
            inputs,
            "transfer-index",
            OperationalErrorKind::ChunkOutOfBounds,
        ),
        NativeHostTransferChunkError::Replay => {
            transfer_not_pending_failure(observed_epoch_ms, inputs)
        }
        NativeHostTransferChunkError::ProtectedMedia => chunk_failure(
            observed_epoch_ms,
            inputs,
            "transfer-protection",
            OperationalErrorKind::ProtectedMediaFailure,
        ),
    }
}

#[cfg(any(windows, test))]
fn serialization_failure(observed_epoch_ms: u64, inputs: &TransferInputs<'_>) -> Vec<Value> {
    chunk_failure(
        observed_epoch_ms,
        inputs,
        "serialization",
        OperationalErrorKind::SerializationContractFailure,
    )
}

fn chunk_failure(
    observed_epoch_ms: u64,
    inputs: &TransferInputs<'_>,
    phase: &str,
    kind: OperationalErrorKind,
) -> Vec<Value> {
    vec![transfer_failure(
        observed_epoch_ms,
        inputs,
        phase,
        kind,
        None,
    )]
}
