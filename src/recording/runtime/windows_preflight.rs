//! Windows recording runtime preflight adapter.
#![cfg(windows)]

use super::plan::RecordingRuntimePlan;
use crate::{
    audio::{PcmFormat, runtime_audio_endpoints},
    capture::{ScreenshotCaptureError, recording_source_bounds},
    recording::{
        RecordingStartRequest,
        encoder::EncoderCapabilities,
        host_runtime::video_encoder_availability,
        quality::RecordingQualityLimits,
        runtime::{
            RecordingRuntimeError,
            plan::{RecordingAudioFacts, RecordingSourceGeometry},
            preflight::{RecordingPreflightFacts, preflight_recording_start},
        },
    },
};

const DEFAULT_AUDIO_FORMAT: PcmFormat = PcmFormat {
    sample_rate_hz: 48_000,
    channels: 2,
};

pub(super) fn windows_recording_runtime_plan(
    request: &RecordingStartRequest,
) -> Result<RecordingRuntimePlan, RecordingRuntimeError> {
    preflight_recording_start(request, &windows_preflight_facts(request)?)
}

fn windows_preflight_facts(
    request: &RecordingStartRequest,
) -> Result<RecordingPreflightFacts, RecordingRuntimeError> {
    let source = recording_source_bounds(&request.source).map_err(source_error)?;
    let encoder = video_encoder_availability();
    Ok(RecordingPreflightFacts {
        source: RecordingSourceGeometry {
            width: source.width,
            height: source.height,
        },
        audio_endpoints: runtime_audio_endpoints(),
        audio_facts: default_audio_facts(),
        encoder_capabilities: EncoderCapabilities {
            max_width: 7680,
            max_height: 4320,
            max_fps: 60,
            hardware_available: encoder.h264_hardware_available,
            software_available: encoder.h264_available,
        },
        quality_limits: RecordingQualityLimits {
            max_fps: 60,
            max_width: 7680,
            max_height: 4320,
            hardware_encoder_available: encoder.h264_hardware_available,
        },
    })
}

const fn default_audio_facts() -> RecordingAudioFacts {
    RecordingAudioFacts {
        target_format: DEFAULT_AUDIO_FORMAT,
        microphone_format: DEFAULT_AUDIO_FORMAT,
        system_loopback_format: DEFAULT_AUDIO_FORMAT,
        microphone_clock_drift_ppm: 0,
        system_loopback_clock_drift_ppm: 0,
    }
}

const fn source_error(error: ScreenshotCaptureError) -> RecordingRuntimeError {
    match error {
        ScreenshotCaptureError::UnsupportedMode
        | ScreenshotCaptureError::UnsupportedDeliveryRoute
        | ScreenshotCaptureError::InvalidSelection
        | ScreenshotCaptureError::RegionSelectionCancelled => {
            RecordingRuntimeError::UnsupportedMode
        }
        ScreenshotCaptureError::CaptureUnavailable
        | ScreenshotCaptureError::RawFrameTooLarge
        | ScreenshotCaptureError::InvalidFrame
        | ScreenshotCaptureError::PngEncodeFailed
        | ScreenshotCaptureError::TransferState
        | ScreenshotCaptureError::FileIo => RecordingRuntimeError::RuntimeUnavailable,
    }
}
