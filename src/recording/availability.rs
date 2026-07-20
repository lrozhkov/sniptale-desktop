//! Recording runtime capability fact.

use super::host_runtime::{
    MediaFoundationAvailability, VideoEncoderAvailability, media_foundation_availability,
    video_encoder_availability,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RecordingRuntimeAvailability {
    Available,
    UnsupportedPlatform,
    EncoderUnavailable,
    RuntimeUnavailable,
}

pub(crate) fn recording_runtime_availability() -> RecordingRuntimeAvailability {
    recording_runtime_availability_from_facts(
        media_foundation_availability(),
        video_encoder_availability(),
        platform_recording_runner_available(),
    )
}

pub(crate) const fn recording_runtime_availability_from_facts(
    encoder: MediaFoundationAvailability,
    video_encoder: VideoEncoderAvailability,
    runner_available: bool,
) -> RecordingRuntimeAvailability {
    match encoder {
        MediaFoundationAvailability::UnsupportedPlatform => {
            RecordingRuntimeAvailability::UnsupportedPlatform
        }
        MediaFoundationAvailability::RuntimeUnavailable => {
            RecordingRuntimeAvailability::EncoderUnavailable
        }
        MediaFoundationAvailability::Available if !video_encoder.h264_available => {
            RecordingRuntimeAvailability::EncoderUnavailable
        }
        MediaFoundationAvailability::Available if runner_available => {
            RecordingRuntimeAvailability::Available
        }
        MediaFoundationAvailability::Available => RecordingRuntimeAvailability::RuntimeUnavailable,
    }
}

#[cfg(windows)]
const fn platform_recording_runner_available() -> bool {
    true
}

#[cfg(not(windows))]
const fn platform_recording_runner_available() -> bool {
    false
}
