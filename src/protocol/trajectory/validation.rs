//! Scalar and closed-shape constraints; stream geometry/order validation is separate.

#[cfg(any(windows, test))]
use super::records::{
    CursorTrajectoryBlock, CursorTrajectoryButton, CursorTrajectoryEvent, CursorTrajectoryRecord,
    CursorTrajectorySample,
};

pub(crate) const MAX_SAFE_INTEGER: u64 = (1_u64 << 53) - 1;
#[cfg(any(windows, test))]
pub(crate) const MAX_BLOCK_RECORDS: usize = 512;
#[cfg(any(windows, test))]
pub(crate) const MAX_BLOCK_BYTES: usize = 65_536;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TrajectoryContractError {
    #[cfg(any(windows, test))]
    Identifier,
    #[cfg(any(windows, test))]
    Geometry,
    #[cfg(any(windows, test))]
    Coordinates,
    #[cfg(any(windows, test))]
    Relation,
    #[cfg(any(windows, test))]
    Number,
    #[cfg(any(windows, test))]
    Order,
    #[cfg(any(windows, test))]
    Bounds,
    Count,
    Format,
    #[cfg(any(windows, test))]
    Malformed,
    #[cfg(any(windows, test))]
    Noncanonical,
    #[cfg(any(windows, test))]
    Oversize,
}

pub(super) fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= super::super::versions::MAX_ID_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
}

pub(crate) fn valid_trajectory_id(value: &str) -> bool {
    value.len() == 32
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

pub(crate) fn valid_trajectory_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(any(windows, test))]
pub(super) fn safe(value: u64) -> bool {
    value <= MAX_SAFE_INTEGER
}

#[cfg(any(windows, test))]
pub(super) fn validate_block_shape(
    block: &CursorTrajectoryBlock,
) -> Result<(), TrajectoryContractError> {
    if block.format_version != 1 {
        return Err(TrajectoryContractError::Format);
    }
    if !valid_id(&block.recording_id) || !valid_trajectory_id(&block.trajectory_id) {
        return Err(TrajectoryContractError::Identifier);
    }
    if !safe(block.sequence) || !safe(block.start_time_us) || !safe(block.end_time_us) {
        return Err(TrajectoryContractError::Number);
    }
    if block.records.is_empty() || block.records.len() > MAX_BLOCK_RECORDS {
        return Err(TrajectoryContractError::Count);
    }
    let mut bounds = None;
    let mut geometry = None;
    for (index, record) in block.records.iter().enumerate() {
        validate_record_shape(record)?;
        if is_trajectory_barrier(record) && index + 1 != block.records.len() {
            return Err(TrajectoryContractError::Order);
        }
        match record {
            CursorTrajectoryRecord::Segment(value) => {
                geometry = Some((value.segment_id, value.width, value.height));
            }
            CursorTrajectoryRecord::Sample(value) => {
                if let Some((id, width, height)) = geometry {
                    if value.segment_id != id {
                        return Err(TrajectoryContractError::Order);
                    }
                    if value.x_q8.is_some_and(|x| x >= u64::from(width) * 256)
                        || value.y_q8.is_some_and(|y| y >= u64::from(height) * 256)
                    {
                        return Err(TrajectoryContractError::Coordinates);
                    }
                }
            }
            CursorTrajectoryRecord::Boundary(_) | CursorTrajectoryRecord::Gap(_) => {}
        }
        let (start, end) = record_bounds(record);
        bounds = Some(
            bounds.map_or((start, end), |(old_start, old_end): (u64, u64)| {
                (old_start.min(start), old_end.max(end))
            }),
        );
    }
    if bounds != Some((block.start_time_us, block.end_time_us)) {
        return Err(TrajectoryContractError::Bounds);
    }
    Ok(())
}

#[cfg(any(windows, test))]
pub(crate) fn record_bounds(record: &CursorTrajectoryRecord) -> (u64, u64) {
    match record {
        CursorTrajectoryRecord::Segment(value) => (value.active_time_us, value.active_time_us),
        CursorTrajectoryRecord::Sample(value) => (value.active_time_us, value.active_time_us),
        CursorTrajectoryRecord::Boundary(value) => (value.active_time_us, value.active_time_us),
        CursorTrajectoryRecord::Gap(value) => (
            value.from_time_us,
            value.to_time_us.unwrap_or(value.from_time_us),
        ),
    }
}

#[cfg(any(windows, test))]
pub(crate) fn is_trajectory_barrier(record: &CursorTrajectoryRecord) -> bool {
    matches!(
        record,
        CursorTrajectoryRecord::Boundary(_) | CursorTrajectoryRecord::Gap(_)
    )
}

#[cfg(any(windows, test))]
pub(crate) fn validate_record_shape(
    record: &CursorTrajectoryRecord,
) -> Result<(), TrajectoryContractError> {
    match record {
        CursorTrajectoryRecord::Segment(value) => {
            if value.width == 0 || value.height == 0 {
                return Err(TrajectoryContractError::Geometry);
            }
            if !safe(value.segment_id)
                || !safe(value.active_time_us)
                || value
                    .clock
                    .timestamp_quantum_us
                    .is_some_and(|value| value == 0 || !safe(value))
                || value
                    .clock
                    .resolution_bound_us
                    .is_some_and(|value| !safe(value))
                || value
                    .clock
                    .mapping_uncertainty_us
                    .is_some_and(|value| !safe(value))
            {
                return Err(TrajectoryContractError::Number);
            }
        }
        CursorTrajectoryRecord::Sample(value) => validate_sample_shape(value)?,
        CursorTrajectoryRecord::Boundary(value) => {
            if !safe(value.segment_id) || !safe(value.active_time_us) {
                return Err(TrajectoryContractError::Number);
            }
        }
        CursorTrajectoryRecord::Gap(value) => {
            if !safe(value.segment_id)
                || !safe(value.from_time_us)
                || value
                    .to_time_us
                    .is_some_and(|end| !safe(end) || end < value.from_time_us)
            {
                return Err(TrajectoryContractError::Number);
            }
            match (
                value.first_lost_observation_seq,
                value.last_lost_observation_seq,
            ) {
                (None, None) => {}
                (Some(first), Some(last)) if safe(first) && safe(last) && first <= last => {}
                _ => return Err(TrajectoryContractError::Order),
            }
        }
    }
    Ok(())
}

#[cfg(any(windows, test))]
fn validate_sample_shape(value: &CursorTrajectorySample) -> Result<(), TrajectoryContractError> {
    if !safe(value.segment_id) || !safe(value.observation_seq) || !safe(value.active_time_us) {
        return Err(TrajectoryContractError::Number);
    }
    match (value.observed, value.x_q8, value.y_q8) {
        (true, Some(x), Some(y))
            if x < u64::from(u32::MAX) * 256 && y < u64::from(u32::MAX) * 256 => {}
        (false, None, None) => {}
        _ => return Err(TrajectoryContractError::Coordinates),
    }
    if value.anchors.len() > 11 || !value.anchors.windows(2).all(|pair| pair[0] < pair[1]) {
        return Err(TrajectoryContractError::Order);
    }
    if value.related_observation_seq.is_some_and(|related| {
        value.event != CursorTrajectoryEvent::Click
            || related >= value.observation_seq
            || !safe(related)
    }) || (value.event == CursorTrajectoryEvent::Move)
        != (value.button == CursorTrajectoryButton::None)
    {
        return Err(TrajectoryContractError::Relation);
    }
    Ok(())
}
