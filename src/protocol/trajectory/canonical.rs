//! Canonical JSON subset; a decoded block must reproduce its exact bytes.

use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;

use super::{
    records::CursorTrajectoryBlock,
    validation::{MAX_BLOCK_BYTES, TrajectoryContractError, validate_block_shape},
};

pub(crate) fn encode_trajectory_block(
    block: &CursorTrajectoryBlock,
) -> Result<Vec<u8>, TrajectoryContractError> {
    validate_block_shape(block)?;
    let bytes = canonical_bytes(block)?;
    if bytes.len() > MAX_BLOCK_BYTES {
        return Err(TrajectoryContractError::Oversize);
    }
    Ok(bytes)
}

pub(crate) fn decode_trajectory_block(
    bytes: &[u8],
) -> Result<CursorTrajectoryBlock, TrajectoryContractError> {
    if bytes.len() > MAX_BLOCK_BYTES {
        return Err(TrajectoryContractError::Oversize);
    }
    let block = serde_json::from_slice(bytes).map_err(|_| TrajectoryContractError::Malformed)?;
    if encode_trajectory_block(&block)? != bytes {
        return Err(TrajectoryContractError::Noncanonical);
    }
    Ok(block)
}

/// Used only for closed trajectory DTOs after their scalar validation.
pub(crate) fn canonical_bytes(value: &impl Serialize) -> Result<Vec<u8>, TrajectoryContractError> {
    let value = serde_json::to_value(value).map_err(|_| TrajectoryContractError::Malformed)?;
    serde_json::to_vec(&sorted_value(value)).map_err(|_| TrajectoryContractError::Malformed)
}

fn sorted_value(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let ordered: BTreeMap<_, _> = map
                .into_iter()
                .map(|(key, value)| (key, sorted_value(value)))
                .collect();
            Value::Object(ordered.into_iter().collect())
        }
        Value::Array(items) => Value::Array(items.into_iter().map(sorted_value).collect()),
        value => value,
    }
}

#[cfg(test)]
pub(crate) fn trajectory_fixture() -> Result<serde_json::Value, serde_json::Error> {
    serde_json::from_slice(include_bytes!(
        "../../../contracts/native/fixtures/cursor-trajectory-v1.json"
    ))
}
