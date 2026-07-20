//! Windows recording runtime facts and host resource probes.

#[cfg(windows)]
mod disk;
mod media_foundation;

#[cfg(windows)]
use std::path::Path;

#[cfg(windows)]
use super::disk_guard::{RecordingDiskGuardError, RecordingDiskSpace};

#[cfg(windows)]
pub(crate) use media_foundation::MediaFoundationRuntime;
pub(crate) use media_foundation::{MediaFoundationAvailability, VideoEncoderAvailability};

pub(crate) fn media_foundation_availability() -> MediaFoundationAvailability {
    media_foundation::media_foundation_availability()
}

pub(crate) fn video_encoder_availability() -> VideoEncoderAvailability {
    media_foundation::video_encoder_availability()
}

#[cfg(windows)]
pub(crate) fn recording_volume_space(
    media_path: &Path,
) -> Result<RecordingDiskSpace, RecordingDiskGuardError> {
    disk::recording_volume_space(media_path)
}
