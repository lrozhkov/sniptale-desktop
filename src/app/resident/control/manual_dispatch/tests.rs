use std::{sync::mpsc, thread};

use tempfile::tempdir;

use super::ManualActionDispatcher;
use crate::{
    app::resident::WorkerCommand,
    protocol::{DeliveryRoute, RecordingControl, VideoMode},
    settings::{
        ActiveSettings, LocalProfileDraft, ManualProfileDraft, apply_local_profile_draft_from_path,
        ensure_local_profile_from_path,
    },
};

#[test]
fn screenshot_waits_for_durable_manual_asset_bind_before_capture_enqueue()
-> Result<(), Box<dyn std::error::Error>> {
    for (target, mode) in [
        (
            crate::tray::desktop::model::CaptureTarget::Display,
            crate::protocol::ScreenshotMode::Screen,
        ),
        (
            crate::tray::desktop::model::CaptureTarget::Window,
            crate::protocol::ScreenshotMode::ActiveWindow,
        ),
    ] {
        check_screenshot_dispatch(target, &mode)?;
    }
    Ok(())
}

fn check_screenshot_dispatch(
    target: crate::tray::desktop::model::CaptureTarget,
    mode: &crate::protocol::ScreenshotMode,
) -> Result<(), Box<dyn std::error::Error>> {
    let (_directory, settings) = prepared_manual_settings()?;
    let (capture_sender, capture_receiver) = mpsc::sync_channel(1);
    let (asset_sender, asset_receiver) = mpsc::sync_channel(1);
    let mut dispatcher =
        ManualActionDispatcher::new(settings, Some(capture_sender), None, Some(asset_sender));
    let projection = crate::app::resident::desktop::projection::CaptureProjection::default();
    dispatcher.observe_with(projection.clone());
    let request_projection = projection.clone();
    let dispatch = thread::spawn(move || {
        crate::app::resident::desktop::dispatch::dispatch_capture_request(
            crate::tray::desktop::model::CaptureRequest::Start {
                options: crate::tray::desktop::model::CaptureOptions::Screenshot { target },
                expected_epoch: 0,
            },
            Some(&mut dispatcher),
            &request_projection,
            crate::tray::TrayRecordingStatus::Idle,
            &mut crate::tray::NoopTrayCommandSource,
        )
    });

    let WorkerCommand::PrepareManualAsset { job, result_sender } = asset_receiver.recv()? else {
        return Err(std::io::Error::other("expected prepared asset").into());
    };
    assert!(matches!(
        job.state(),
        crate::assets::AssetJobState::Prepared
    ));
    assert!(capture_receiver.try_recv().is_err());
    let snapshot = projection
        .snapshot(
            crate::app::resident::desktop::projection::tests::capabilities(),
            crate::tray::desktop::model::CaptureQuality::Balanced,
        )
        .map_err(|_| std::io::Error::other("projection"))?;
    assert_eq!(
        snapshot.phase,
        crate::tray::desktop::model::CapturePhase::Starting
    );
    assert_eq!(snapshot.outcome, None);
    result_sender.send(Ok(()))?;
    dispatch
        .join()
        .map_err(|_| std::io::Error::other("dispatch thread"))?
        .map_err(|_| std::io::Error::other("dispatch"))?;
    let WorkerCommand::CaptureScreenshot {
        request,
        extension_controller_lease_id,
    } = capture_receiver.recv()?
    else {
        return Err(std::io::Error::other("expected capture").into());
    };
    assert_eq!(extension_controller_lease_id, None);
    assert_eq!(&request.mode, mode);
    assert!(matches!(
        request.delivery_route,
        DeliveryRoute::DesktopDelivery {
            target_id: None,
            ask_after_capture: false,
            ..
        }
    ));
    Ok(())
}

#[test]
fn recording_safety_control_uses_the_local_recording_worker()
-> Result<(), Box<dyn std::error::Error>> {
    let (_directory, settings) = prepared_manual_settings()?;
    let (recording_sender, recording_receiver) = mpsc::sync_channel(2);
    let (asset_sender, asset_receiver) = mpsc::sync_channel(1);
    let mut dispatcher =
        ManualActionDispatcher::new(settings, None, Some(recording_sender), Some(asset_sender));
    let start = thread::spawn(move || {
        dispatcher.enqueue_recording(VideoMode::Screen)?;
        dispatcher.control_recording(&RecordingControl::Pause)?;
        Ok::<_, super::ManualDispatchError>(())
    });
    let WorkerCommand::PrepareManualAsset { result_sender, .. } = asset_receiver.recv()? else {
        return Err(std::io::Error::other("expected prepared asset").into());
    };
    result_sender.send(Ok(()))?;
    let WorkerCommand::StartRecording { .. } = recording_receiver.recv()? else {
        return Err(std::io::Error::other("expected recording start").into());
    };
    let WorkerCommand::ControlActiveRecording { control } = recording_receiver.recv()? else {
        return Err(std::io::Error::other("expected local recording control").into());
    };
    assert_eq!(control, RecordingControl::Pause);
    start
        .join()
        .map_err(|_| std::io::Error::other("dispatch thread"))?
        .map_err(|_| std::io::Error::other("dispatch"))?;
    Ok(())
}

#[test]
fn ui_controls_reject_stale_operation_epochs_without_retargeting_the_worker()
-> Result<(), Box<dyn std::error::Error>> {
    use crate::tray::desktop::model::{CaptureKind, CapturePhase, CaptureRequest, DesktopError};
    let directory = tempdir()?;
    let profile = ensure_local_profile_from_path(Some(directory.path().join("profile.json")))
        .map_err(|_| std::io::Error::other("profile"))?;
    let (sender, receiver) = mpsc::sync_channel(1);
    let mut dispatcher = ManualActionDispatcher::new(
        ActiveSettings::from_profile(&profile),
        None,
        Some(sender),
        None,
    );
    let projection = crate::app::resident::desktop::projection::CaptureProjection::default();
    projection.begin(
        "one",
        Some("job-one"),
        CaptureKind::Video,
        CapturePhase::Starting,
        Some("recording-one"),
    );
    projection.recording("recording-one", CapturePhase::Recording, 0);
    let mut source = crate::tray::NoopTrayCommandSource;
    crate::app::resident::desktop::dispatch::dispatch_capture_request(
        CaptureRequest::Pause { expected_epoch: 1 },
        Some(&mut dispatcher),
        &projection,
        crate::tray::TrayRecordingStatus::Recording,
        &mut source,
    )
    .map_err(|_| std::io::Error::other("control"))?;
    assert!(matches!(
        receiver.try_recv()?,
        WorkerCommand::ControlObservedRecording {
            control: RecordingControl::Pause, recording_id,
        } if recording_id == "recording-one"
    ));
    projection.recording_discarded("recording-one");
    projection.begin(
        "two",
        Some("job-two"),
        CaptureKind::Video,
        CapturePhase::Starting,
        Some("recording-two"),
    );
    projection.recording("recording-two", CapturePhase::Recording, 0);
    let result = crate::app::resident::desktop::dispatch::dispatch_capture_request(
        CaptureRequest::Discard { expected_epoch: 1 },
        Some(&mut dispatcher),
        &projection,
        crate::tray::TrayRecordingStatus::Recording,
        &mut source,
    );
    assert_eq!(result, Err(DesktopError::Rejected));
    assert!(receiver.try_recv().is_err());
    Ok(())
}

#[test]
fn ui_recording_quality_is_frozen_without_mutating_the_profile()
-> Result<(), Box<dyn std::error::Error>> {
    use crate::tray::desktop::model::{
        CaptureOptions, CaptureQuality, CaptureRequest, CaptureTarget,
    };
    let (_directory, settings) = prepared_manual_settings()?;
    let before = settings
        .profile_snapshot()
        .map_err(|_| std::io::Error::other("snapshot"))?
        .manual_recording_settings()
        .ok_or_else(|| std::io::Error::other("recording settings"))?
        .requested_quality;
    let (sender, receiver) = mpsc::sync_channel(1);
    let (assets, prepare) = mpsc::sync_channel(1);
    let mut dispatcher =
        ManualActionDispatcher::new(settings.clone(), None, Some(sender), Some(assets));
    let task = thread::spawn(move || {
        crate::app::resident::desktop::dispatch::dispatch_capture_request(
            CaptureRequest::Start {
                options: CaptureOptions::Video {
                    target: CaptureTarget::Window,
                    quality: CaptureQuality::High,
                },
                expected_epoch: 0,
            },
            Some(&mut dispatcher),
            &crate::app::resident::desktop::projection::CaptureProjection::default(),
            crate::tray::TrayRecordingStatus::Idle,
            &mut crate::tray::NoopTrayCommandSource,
        )
    });
    let WorkerCommand::PrepareManualAsset { result_sender, .. } = prepare.recv()? else {
        return Err(std::io::Error::other("prepare").into());
    };
    result_sender.send(Ok(()))?;
    task.join()
        .map_err(|_| std::io::Error::other("thread"))?
        .map_err(|_| std::io::Error::other("dispatch"))?;
    let WorkerCommand::StartRecording { request, .. } = receiver.recv()? else {
        return Err(std::io::Error::other("recording").into());
    };
    assert_eq!(request.source.mode, VideoMode::ActiveWindow);
    assert_eq!(
        request.requested_quality.quality,
        crate::protocol::NativeVideoQuality::High
    );
    let after = settings
        .profile_snapshot()
        .map_err(|_| std::io::Error::other("snapshot"))?
        .manual_recording_settings()
        .ok_or_else(|| std::io::Error::other("recording settings"))?
        .requested_quality;
    assert_eq!(after, before);
    Ok(())
}

fn prepared_manual_settings()
-> Result<(tempfile::TempDir, ActiveSettings), Box<dyn std::error::Error>> {
    let directory = tempdir()?;
    let path = directory.path().join("profile.json");
    let profile = ensure_local_profile_from_path(Some(path.clone()))
        .map_err(|_| std::io::Error::other("profile"))?;
    let settings = ActiveSettings::from_profile(&profile);
    let draft = LocalProfileDraft::manual(ManualProfileDraft::with_defaults(
        "destination-1".to_owned(),
        directory.path().to_string_lossy().into_owned(),
    ));
    apply_local_profile_draft_from_path(Some(path), &settings, &draft)
        .map_err(|_| std::io::Error::other("apply"))?;
    Ok((directory, settings))
}

#[derive(Default)]
struct MissingWindowSource(crate::tray::NoopTrayCommandSource);
impl crate::tray::TrayCommandSource for MissingWindowSource {
    fn prepare_capture(
        &mut self,
        _action: crate::tray::ShellActionId,
    ) -> Result<(), crate::tray::TrayRuntimeError> {
        Err(crate::tray::TrayRuntimeError::NoCaptureWindow)
    }
    fn apply_render_plan(
        &mut self,
        plan: &crate::tray::TrayMenuRenderPlan,
        tooltip: &str,
        icon: crate::tray::TrayIconVariant,
    ) -> Result<(), crate::tray::TrayRuntimeError> {
        self.0.apply_render_plan(plan, tooltip, icon)
    }
    fn drain_events(
        &mut self,
    ) -> Result<Vec<crate::tray::TrayRuntimeEvent>, crate::tray::TrayRuntimeError> {
        self.0.drain_events()
    }
    fn show_notice(
        &mut self,
        kind: crate::tray::TrayNoticeKind,
    ) -> Result<(), crate::tray::TrayRuntimeError> {
        self.0.show_notice(kind)
    }
    fn show_settings_window(
        &mut self,
        presentation: crate::tray::settings_window::model::SettingsWindowPresentation,
    ) -> Result<(), crate::tray::TrayRuntimeError> {
        self.0.show_settings_window(presentation)
    }
    fn hide_settings_window(&mut self) -> Result<(), crate::tray::TrayRuntimeError> {
        self.0.hide_settings_window()
    }
    fn open_save_folder(
        &mut self,
        destination: &std::path::Path,
    ) -> Result<(), crate::tray::TrayRuntimeError> {
        self.0.open_save_folder(destination)
    }
}

#[test]
fn vanished_window_refuses_before_asset_or_capture_side_effects()
-> Result<(), Box<dyn std::error::Error>> {
    use crate::tray::desktop::model::{
        CaptureOptions, CaptureRequest, CaptureTarget, DesktopError,
    };
    let (_directory, settings) = prepared_manual_settings()?;
    let (capture, captures) = mpsc::sync_channel(1);
    let (assets, preparations) = mpsc::sync_channel(1);
    let mut dispatcher = ManualActionDispatcher::new(settings, Some(capture), None, Some(assets));
    let projection = crate::app::resident::desktop::projection::CaptureProjection::default();
    assert_eq!(
        crate::app::resident::desktop::dispatch::dispatch_capture_request(
            CaptureRequest::Start {
                options: CaptureOptions::Screenshot {
                    target: CaptureTarget::Window
                },
                expected_epoch: 0
            },
            Some(&mut dispatcher),
            &projection,
            crate::tray::TrayRecordingStatus::Idle,
            &mut MissingWindowSource::default(),
        ),
        Err(DesktopError::SourceUnavailable)
    );
    assert!(captures.try_recv().is_err());
    assert!(preparations.try_recv().is_err());
    assert!(projection.epoch_matches(0));
    Ok(())
}
