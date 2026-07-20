use std::{sync::mpsc, thread};

use tempfile::tempdir;

use super::ManualActionDispatcher;
use crate::{
    app::resident::WorkerCommand,
    protocol::{DeliveryRoute, RecordingControl, ScreenshotMode, VideoMode},
    settings::{
        ActiveSettings, LocalProfileDraft, ManualProfileDraft, apply_local_profile_draft_from_path,
        ensure_local_profile_from_path,
    },
};

#[test]
fn screenshot_waits_for_durable_manual_asset_bind_before_capture_enqueue()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempdir()?;
    let profile_path = directory.path().join("local-profile.json");
    let profile = ensure_local_profile_from_path(Some(profile_path.clone()))
        .map_err(|_| std::io::Error::other("profile"))?;
    let settings = ActiveSettings::from_profile(&profile);
    let draft = LocalProfileDraft::manual(ManualProfileDraft::with_defaults(
        "destination-1".to_owned(),
        directory.path().to_string_lossy().into_owned(),
    ));
    apply_local_profile_draft_from_path(Some(profile_path), &settings, &draft)
        .map_err(|_| std::io::Error::other("apply"))?;
    let (capture_sender, capture_receiver) = mpsc::sync_channel(1);
    let (asset_sender, asset_receiver) = mpsc::sync_channel(1);
    let mut dispatcher =
        ManualActionDispatcher::new(settings, Some(capture_sender), None, Some(asset_sender));
    let dispatch = thread::spawn(move || dispatcher.enqueue_screenshot(ScreenshotMode::Screen));

    let WorkerCommand::PrepareManualAsset { job, result_sender } = asset_receiver.recv()? else {
        return Err(std::io::Error::other("expected prepared asset").into());
    };
    assert!(matches!(
        job.state(),
        crate::assets::AssetJobState::Prepared
    ));
    assert!(capture_receiver.try_recv().is_err());
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
    assert!(matches!(
        request.delivery_route,
        DeliveryRoute::LocalAsset { .. }
    ));
    Ok(())
}

#[test]
fn recording_safety_control_uses_the_local_recording_worker()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempdir()?;
    let profile_path = directory.path().join("local-profile.json");
    let profile = ensure_local_profile_from_path(Some(profile_path.clone()))
        .map_err(|_| std::io::Error::other("profile"))?;
    let settings = ActiveSettings::from_profile(&profile);
    let draft = LocalProfileDraft::manual(ManualProfileDraft::with_defaults(
        "destination-1".to_owned(),
        directory.path().to_string_lossy().into_owned(),
    ));
    apply_local_profile_draft_from_path(Some(profile_path), &settings, &draft)
        .map_err(|_| std::io::Error::other("apply"))?;
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
    let WorkerCommand::StartRecording { request, .. } = recording_receiver.recv()? else {
        return Err(std::io::Error::other("expected recording start").into());
    };
    let WorkerCommand::ControlRecording { request: control } = recording_receiver.recv()? else {
        return Err(std::io::Error::other("expected local recording control").into());
    };
    assert_eq!(
        control.recording_id,
        crate::transfer::identity::derived_media_id("recording", &request.operation_id)
    );
    assert_eq!(control.delivery_session.controller_lease_id, None);
    assert_eq!(control.control, RecordingControl::Pause);
    start
        .join()
        .map_err(|_| std::io::Error::other("dispatch thread"))?
        .map_err(|_| std::io::Error::other("dispatch"))?;
    Ok(())
}
