//! Scalar descriptor constraints; durable receipts must additionally match their owner ledger.

use super::{
    CursorTrajectoryDescriptor, CursorTrajectoryState, MAX_SAFE_INTEGER, TrajectoryContractError,
    valid_trajectory_hash, valid_trajectory_id,
};

pub(crate) fn validate_trajectory_descriptor(
    value: &CursorTrajectoryDescriptor,
) -> Result<(), TrajectoryContractError> {
    if value.format_version != 1
        || !valid_trajectory_id(&value.trajectory_id)
        || !valid_trajectory_hash(&value.stream_sha256)
    {
        return Err(TrajectoryContractError::Format);
    }
    let counts = [
        value.total_blocks,
        value.total_records,
        value.total_samples,
        value.total_bytes,
    ];
    if counts.into_iter().any(|count| count > MAX_SAFE_INTEGER)
        || value.total_samples > value.total_records
        || value.total_blocks > value.total_records
        || u128::from(value.total_records) > u128::from(value.total_blocks) * 512
        || u128::from(value.total_bytes) > u128::from(value.total_blocks) * 65536
        || value.total_bytes < value.total_blocks
        || (value.total_blocks == 0
            && (value.total_bytes != 0
                || value.total_records != 0
                || value.stream_sha256
                    != "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"))
        || (value.state == CursorTrajectoryState::Unavailable && !value.finalized)
    {
        return Err(TrajectoryContractError::Count);
    }
    Ok(())
}
