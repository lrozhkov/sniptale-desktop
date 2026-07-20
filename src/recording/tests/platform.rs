use super::*;
use crate::recording::RecordingRuntimeAvailability;
#[cfg(windows)]
use crate::recording::runtime;

#[cfg(not(windows))]
#[test]
fn media_foundation_availability_fails_closed_off_windows() {
    assert_eq!(
        media_foundation_availability(),
        MediaFoundationAvailability::UnsupportedPlatform
    );
}

#[cfg(not(windows))]
#[test]
fn video_encoder_availability_fails_closed_off_windows() {
    assert_eq!(
        video_encoder_availability(),
        VideoEncoderAvailability {
            h264_available: false,
            h264_hardware_available: false,
        }
    );
}

#[test]
fn recording_runtime_availability_requires_encoder_and_runner() {
    let available_encoder = VideoEncoderAvailability {
        h264_available: true,
        h264_hardware_available: false,
    };
    let missing_encoder = VideoEncoderAvailability {
        h264_available: false,
        h264_hardware_available: false,
    };

    assert_eq!(
        recording_runtime_availability_from_facts(
            MediaFoundationAvailability::Available,
            available_encoder,
            true
        ),
        RecordingRuntimeAvailability::Available
    );
    assert_eq!(
        recording_runtime_availability_from_facts(
            MediaFoundationAvailability::Available,
            available_encoder,
            false
        ),
        RecordingRuntimeAvailability::RuntimeUnavailable
    );
    assert_eq!(
        recording_runtime_availability_from_facts(
            MediaFoundationAvailability::Available,
            missing_encoder,
            true
        ),
        RecordingRuntimeAvailability::EncoderUnavailable
    );
    assert_eq!(
        recording_runtime_availability_from_facts(
            MediaFoundationAvailability::RuntimeUnavailable,
            available_encoder,
            true
        ),
        RecordingRuntimeAvailability::EncoderUnavailable
    );
    assert_eq!(
        recording_runtime_availability_from_facts(
            MediaFoundationAvailability::UnsupportedPlatform,
            available_encoder,
            true
        ),
        RecordingRuntimeAvailability::UnsupportedPlatform
    );
}

#[cfg(windows)]
#[test]
fn default_recording_runner_is_wired_on_windows() -> Result<(), String> {
    let temp = tempfile::tempdir().map_err(|error| error.to_string())?;
    let store = RecordingTransferStore::open_with_default_config(temp.path())
        .map_err(|error| format!("{error:?}"))?;

    assert!(runtime::default_recording_runner(store).is_some());
    Ok(())
}
