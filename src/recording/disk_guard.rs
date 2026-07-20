//! Recording disk pressure guard.
const MIB: u64 = 1024 * 1024;
const MIN_RESERVE_BYTES: u64 = 512 * MIB;
const MAX_RESERVE_BYTES: u64 = 2 * 1024 * MIB;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RecordingDiskSpace {
    pub(crate) available_bytes: u64,
    pub(crate) capacity_bytes: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RecordingDiskGuardError {
    ProbeFailed,
    InvalidCapacity,
    DiskPressure {
        available_bytes: u64,
        required_bytes: u64,
    },
}

pub(crate) type RecordingDiskSpaceProbe =
    fn(&std::path::Path) -> Result<RecordingDiskSpace, RecordingDiskGuardError>;

pub(crate) fn ensure_recording_disk_headroom(
    space: RecordingDiskSpace,
    planned_recording_bytes: u64,
) -> Result<(), RecordingDiskGuardError> {
    let reserve = recording_disk_reserve_bytes(space.capacity_bytes)?;
    let required = reserve
        .checked_add(planned_recording_bytes)
        .ok_or(RecordingDiskGuardError::InvalidCapacity)?;
    if space.available_bytes < required {
        return Err(RecordingDiskGuardError::DiskPressure {
            available_bytes: space.available_bytes,
            required_bytes: required,
        });
    }
    Ok(())
}

pub(crate) fn recording_disk_reserve_bytes(
    capacity_bytes: u64,
) -> Result<u64, RecordingDiskGuardError> {
    if capacity_bytes == 0 {
        return Err(RecordingDiskGuardError::InvalidCapacity);
    }
    Ok((capacity_bytes / 10).clamp(MIN_RESERVE_BYTES, MAX_RESERVE_BYTES))
}
