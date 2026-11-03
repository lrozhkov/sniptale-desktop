//! Local UI admission before existing Manual coordination and recording-owner controls.

use super::projection::CaptureProjection;
use crate::{
    app::resident::control::manual_dispatch::{ManualActionDispatcher, ManualDispatchError},
    protocol::{RecordingControl, ScreenshotMode, VideoMode, recording::NativeVideoQuality},
    tray::{
        ShellActionId, TrayCommandSource, TrayRecordingStatus,
        desktop::model::{
            CaptureOptions, CapturePhase, CaptureQuality, CaptureRequest, CaptureTarget,
            DesktopError,
        },
    },
};

pub(crate) fn dispatch_capture_request(
    request: CaptureRequest,
    dispatcher: Option<&mut ManualActionDispatcher>,
    projection: &CaptureProjection,
    recording: TrayRecordingStatus,
    source: &mut impl TrayCommandSource,
) -> Result<(), DesktopError> {
    let expected = match request {
        CaptureRequest::Start { expected_epoch, .. }
        | CaptureRequest::Pause { expected_epoch }
        | CaptureRequest::Resume { expected_epoch }
        | CaptureRequest::Stop { expected_epoch }
        | CaptureRequest::Discard { expected_epoch } => expected_epoch,
    };
    if !projection.epoch_matches(expected) {
        return Err(DesktopError::Rejected);
    }
    let phase = projection.phase()?;
    let dispatcher = dispatcher.ok_or(DesktopError::Unavailable)?;
    dispatcher.observe_with(projection.clone());
    match request {
        CaptureRequest::Start { options, .. } => {
            if recording != TrayRecordingStatus::Idle
                || !matches!(
                    phase,
                    CapturePhase::Idle | CapturePhase::Complete | CapturePhase::Failed
                )
            {
                return Err(DesktopError::Busy);
            }
            start(options, dispatcher, source)
        }
        CaptureRequest::Pause { .. }
            if recording == TrayRecordingStatus::Recording && phase == CapturePhase::Recording =>
        {
            control(dispatcher, projection, expected, &RecordingControl::Pause)
        }
        CaptureRequest::Resume { .. }
            if recording == TrayRecordingStatus::Paused && phase == CapturePhase::Paused =>
        {
            control(dispatcher, projection, expected, &RecordingControl::Resume)
        }
        CaptureRequest::Stop { .. } | CaptureRequest::Discard { .. }
            if matches!(
                recording,
                TrayRecordingStatus::Recording | TrayRecordingStatus::Paused
            ) && matches!(phase, CapturePhase::Recording | CapturePhase::Paused) =>
        {
            control(
                dispatcher,
                projection,
                expected,
                &if matches!(request, CaptureRequest::Stop { .. }) {
                    RecordingControl::Stop
                } else {
                    RecordingControl::Cancel
                },
            )
        }
        _ => Err(DesktopError::Rejected),
    }
}

fn control(
    dispatcher: &mut ManualActionDispatcher,
    projection: &CaptureProjection,
    expected_epoch: u64,
    control: &RecordingControl,
) -> Result<(), DesktopError> {
    let recording_id = projection.recording_identity(expected_epoch)?;
    dispatcher
        .control_observed_recording(control, recording_id)
        .map_err(dispatch_error)
}

fn start(
    options: CaptureOptions,
    dispatcher: &mut ManualActionDispatcher,
    source: &mut impl TrayCommandSource,
) -> Result<(), DesktopError> {
    let target = match options {
        CaptureOptions::Screenshot { target } | CaptureOptions::Video { target, .. } => target,
    };
    let action = match options {
        CaptureOptions::Screenshot {
            target: CaptureTarget::Region,
        } => ShellActionId::ScreenshotRegion,
        CaptureOptions::Screenshot {
            target: CaptureTarget::Window,
        } => ShellActionId::ScreenshotActiveWindow,
        CaptureOptions::Screenshot { .. } => ShellActionId::ScreenshotScreen,
        CaptureOptions::Video {
            target: CaptureTarget::Region,
            ..
        } => ShellActionId::RecordingStartRegion,
        CaptureOptions::Video {
            target: CaptureTarget::Window,
            ..
        } => ShellActionId::RecordingStartActiveWindow,
        CaptureOptions::Video { .. } => ShellActionId::RecordingStartScreen,
    };
    source.prepare_capture(action).map_err(|error| {
        if error == crate::tray::TrayRuntimeError::NoCaptureWindow {
            DesktopError::SourceUnavailable
        } else {
            DesktopError::Unavailable
        }
    })?;
    match options {
        CaptureOptions::Screenshot { .. } => dispatcher
            .enqueue_screenshot(match target {
                CaptureTarget::Region => ScreenshotMode::Region,
                CaptureTarget::Window => ScreenshotMode::ActiveWindow,
                CaptureTarget::Display => ScreenshotMode::Screen,
            })
            .map_err(dispatch_error),
        CaptureOptions::Video { quality, .. } => {
            let quality = match quality {
                CaptureQuality::Low => NativeVideoQuality::Low,
                CaptureQuality::Balanced => NativeVideoQuality::Medium,
                CaptureQuality::High => NativeVideoQuality::High,
                CaptureQuality::Ultra => NativeVideoQuality::Ultra,
            };
            dispatcher
                .enqueue_recording_with_quality(
                    match target {
                        CaptureTarget::Region => VideoMode::Region,
                        CaptureTarget::Window => VideoMode::ActiveWindow,
                        CaptureTarget::Display => VideoMode::Screen,
                    },
                    Some(quality),
                )
                .map_err(dispatch_error)
        }
    }
}

fn dispatch_error(error: ManualDispatchError) -> DesktopError {
    match error {
        ManualDispatchError::WorkerBusy => DesktopError::Busy,
        ManualDispatchError::WorkerUnavailable
        | ManualDispatchError::AssetPrepareFailed
        | ManualDispatchError::EntropyUnavailable => DesktopError::Unavailable,
        ManualDispatchError::DestinationRequired | ManualDispatchError::ModeUnavailable => {
            DesktopError::Rejected
        }
        #[cfg(windows)]
        ManualDispatchError::RegionCancelled => DesktopError::Rejected,
    }
}
