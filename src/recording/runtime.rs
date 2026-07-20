//! Recording runtime platform adapter seam.
#[cfg(any(windows, test))]
mod audio_source;
#[cfg(any(windows, test))]
mod frame_clock;
pub(crate) mod plan;
pub(crate) mod preflight;
#[cfg(windows)]
mod windows_audio;
#[cfg(windows)]
mod windows_preflight;
#[cfg(windows)]
mod windows_runtime;
#[cfg(windows)]
mod windows_video;
pub(crate) mod worker;

use std::path::Path;

use crate::protocol::NativeEffectiveQualitySettings;
#[cfg(test)]
use crate::protocol::NativeRequestedQualitySettings;
#[cfg(windows)]
use crate::telemetry::RecordingTelemetrySource;
#[cfg(all(windows, test))]
use crate::telemetry::default_recording_telemetry_source;

#[cfg(any(windows, test))]
use super::RecordingStartRequest;
#[cfg(test)]
use super::quality::{RecordingQualityError, RecordingQualityLimits, effective_quality};
#[cfg(windows)]
use super::{
    RecordingRunner, service::RecordingLifecycleService, transfer_store::RecordingTransferStore,
};

#[cfg(windows)]
pub(crate) use windows_runtime::WindowsRecordingRuntime;

#[cfg(all(windows, test))]
pub(crate) fn default_recording_runner(
    transfer_store: RecordingTransferStore,
) -> Option<Box<dyn RecordingRunner>> {
    default_recording_runner_with_telemetry(transfer_store, default_recording_telemetry_source())
}

#[cfg(windows)]
pub(crate) fn default_recording_runner_with_telemetry<T>(
    transfer_store: RecordingTransferStore,
    telemetry: T,
) -> Option<Box<dyn RecordingRunner>>
where
    T: RecordingTelemetrySource + 'static,
{
    Some(Box::new(RecordingLifecycleService::new_with_telemetry(
        transfer_store,
        WindowsRecordingRuntime::default(),
        telemetry,
    )))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RecordingRuntimeError {
    RuntimeUnavailable,
    UnsupportedMode,
    InvalidState,
    RuntimeFailed,
    #[cfg(windows)]
    RuntimeFailure(RecordingRuntimeFailure),
    DiskPressure,
    TransferState,
}

#[cfg(windows)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RecordingRuntimeFailure {
    WorkerPanicked,
    SinkPlan,
    SinkCreate(Option<i32>),
    VideoSource,
    VideoPoll(i32),
    VideoTimestamp(i32),
    VideoSurface(i32),
    VideoInterface(i32),
    VideoReadback,
    VideoComposite,
    VideoGeometry,
    VideoClock,
    AudioSource,
    SinkSample,
    SinkWrite(Option<i32>),
    SinkFinalize(Option<i32>),
}

#[cfg(all(windows, not(test)))]
impl RecordingRuntimeError {
    pub(crate) const fn diagnostic_fields(self) -> (&'static str, Option<i32>) {
        match self {
            Self::RuntimeFailure(RecordingRuntimeFailure::WorkerPanicked) => {
                ("recording.worker.panicked", None)
            }
            Self::RuntimeFailure(RecordingRuntimeFailure::SinkPlan) => {
                ("recording.sink.plan", None)
            }
            Self::RuntimeFailure(RecordingRuntimeFailure::SinkCreate(code)) => {
                ("recording.sink.create", code)
            }
            Self::RuntimeFailure(RecordingRuntimeFailure::VideoSource) => {
                ("recording.video.source", None)
            }
            Self::RuntimeFailure(RecordingRuntimeFailure::VideoPoll(code)) => {
                ("recording.video.poll", Some(code))
            }
            Self::RuntimeFailure(RecordingRuntimeFailure::VideoTimestamp(code)) => {
                ("recording.video.timestamp", Some(code))
            }
            Self::RuntimeFailure(RecordingRuntimeFailure::VideoSurface(code)) => {
                ("recording.video.surface", Some(code))
            }
            Self::RuntimeFailure(RecordingRuntimeFailure::VideoInterface(code)) => {
                ("recording.video.interface", Some(code))
            }
            Self::RuntimeFailure(RecordingRuntimeFailure::VideoReadback) => {
                ("recording.video.readback", None)
            }
            Self::RuntimeFailure(RecordingRuntimeFailure::VideoComposite) => {
                ("recording.video.composite", None)
            }
            Self::RuntimeFailure(RecordingRuntimeFailure::VideoGeometry) => {
                ("recording.video.geometry", None)
            }
            Self::RuntimeFailure(RecordingRuntimeFailure::VideoClock) => {
                ("recording.video.clock", None)
            }
            Self::RuntimeFailure(RecordingRuntimeFailure::AudioSource) => {
                ("recording.audio.source", None)
            }
            Self::RuntimeFailure(RecordingRuntimeFailure::SinkSample) => {
                ("recording.sink.sample", None)
            }
            Self::RuntimeFailure(RecordingRuntimeFailure::SinkWrite(code)) => {
                ("recording.sink.write", code)
            }
            Self::RuntimeFailure(RecordingRuntimeFailure::SinkFinalize(code)) => {
                ("recording.sink.finalize", code)
            }
            Self::RuntimeUnavailable => ("recording.runtime.unavailable", None),
            Self::UnsupportedMode => ("recording.mode.unsupported", None),
            Self::InvalidState => ("recording.state.invalid", None),
            Self::RuntimeFailed => ("recording.runtime.failed", None),
            Self::DiskPressure => ("recording.disk.pressure", None),
            Self::TransferState => ("recording.transfer.state", None),
        }
    }
}

pub(crate) struct RecordingRuntimeStartRequest<'a> {
    pub(crate) request: &'a RecordingStartRequest,
    pub(crate) recording_id: &'a str,
    pub(crate) media_path: &'a Path,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct StartedRecordingRuntime {
    pub(crate) effective_quality: NativeEffectiveQualitySettings,
}

pub(crate) struct RecordingRuntimeStopRequest<'a> {
    pub(crate) recording_id: &'a str,
    #[cfg(test)]
    pub(crate) media_path: &'a Path,
    #[cfg(test)]
    pub(crate) requested_at_epoch_ms: u64,
    pub(crate) duration_ms: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct FinalizedRecordingArtifact {
    pub(crate) duration_ms: u64,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) fps: u16,
}

pub(crate) trait RecordingRuntime {
    fn start_recording(
        &mut self,
        request: RecordingRuntimeStartRequest<'_>,
    ) -> Result<StartedRecordingRuntime, RecordingRuntimeError>;

    fn stop_recording(
        &mut self,
        request: RecordingRuntimeStopRequest<'_>,
    ) -> Result<FinalizedRecordingArtifact, RecordingRuntimeError>;

    fn pause_recording(&mut self, recording_id: &str) -> Result<(), RecordingRuntimeError>;

    fn resume_recording(&mut self, recording_id: &str) -> Result<(), RecordingRuntimeError>;

    fn cancel_recording(&mut self, recording_id: &str) -> Result<(), RecordingRuntimeError>;
}

#[cfg(test)]
pub(crate) fn effective_quality_from_settings_policy(
    requested: &NativeRequestedQualitySettings,
    source_width: u32,
    source_height: u32,
    limits: RecordingQualityLimits,
) -> Result<NativeEffectiveQualitySettings, RecordingRuntimeError> {
    effective_quality(requested, source_width, source_height, limits).map_err(|error| match error {
        RecordingQualityError::InvalidNumber => RecordingRuntimeError::RuntimeFailed,
        RecordingQualityError::InvalidSourceGeometry
        | RecordingQualityError::SourceExceedsCapability
        | RecordingQualityError::InvalidManualBitrate => RecordingRuntimeError::UnsupportedMode,
    })
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[cfg(test)]
pub(crate) struct UnsupportedRecordingRuntime;

#[cfg(test)]
impl RecordingRuntime for UnsupportedRecordingRuntime {
    fn start_recording(
        &mut self,
        _request: RecordingRuntimeStartRequest<'_>,
    ) -> Result<StartedRecordingRuntime, RecordingRuntimeError> {
        Err(RecordingRuntimeError::RuntimeUnavailable)
    }

    fn stop_recording(
        &mut self,
        _request: RecordingRuntimeStopRequest<'_>,
    ) -> Result<FinalizedRecordingArtifact, RecordingRuntimeError> {
        Err(RecordingRuntimeError::RuntimeUnavailable)
    }

    fn pause_recording(&mut self, _recording_id: &str) -> Result<(), RecordingRuntimeError> {
        Err(RecordingRuntimeError::RuntimeUnavailable)
    }

    fn resume_recording(&mut self, _recording_id: &str) -> Result<(), RecordingRuntimeError> {
        Err(RecordingRuntimeError::RuntimeUnavailable)
    }

    fn cancel_recording(&mut self, _recording_id: &str) -> Result<(), RecordingRuntimeError> {
        Err(RecordingRuntimeError::RuntimeUnavailable)
    }
}
