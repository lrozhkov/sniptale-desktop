use super::*;
use crate::tray::desktop::model::{CaptureOptions, CaptureTarget};

fn backend(sender: SyncSender<WorkerCommand>) -> ResidentDesktopBackend {
    ResidentDesktopBackend::new(
        ActiveSettings::extension_managed_default(),
        CaptureProjection::default(),
        sender,
        true,
    )
}
fn request() -> CaptureRequest {
    CaptureRequest::Start {
        options: CaptureOptions::Screenshot {
            target: CaptureTarget::Region,
        },
        expected_epoch: 0,
    }
}

#[test]
fn ui_request_uses_existing_queue_and_returns_owner_refusal() -> Result<(), String> {
    let (sender, receiver) = mpsc::sync_channel(1);
    let backend = std::sync::Arc::new(backend(sender));
    let caller = backend.clone();
    let join = std::thread::spawn(move || caller.command(request()));
    let command = receiver.recv().map_err(|_| "missing request")?;
    let WorkerCommand::DesktopCapture {
        request: actual,
        result_sender,
    } = command
    else {
        return Err("wrong command".into());
    };
    assert_eq!(actual, request());
    assert_eq!(backend.command(request()), Err(DesktopError::Busy));
    result_sender
        .send(Err(DesktopError::Rejected))
        .map_err(|_| "missing caller")?;
    assert_eq!(
        join.join().map_err(|_| "caller panicked")?,
        Err(DesktopError::Rejected)
    );
    Ok(())
}

#[test]
fn full_or_closed_queue_refuses_without_deferred_capture() -> Result<(), String> {
    let (sender, receiver) = mpsc::sync_channel(1);
    sender
        .send(WorkerCommand::Stop)
        .map_err(|_| "queue closed")?;
    let backend = backend(sender);
    assert_eq!(backend.command(request()), Err(DesktopError::Busy));
    assert!(matches!(receiver.recv(), Ok(WorkerCommand::Stop)));
    assert!(matches!(
        receiver.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
    drop(receiver);
    assert_eq!(backend.command(request()), Err(DesktopError::Unavailable));
    Ok(())
}

#[test]
fn lost_owner_reply_does_not_report_success() -> Result<(), String> {
    let (sender, receiver) = mpsc::sync_channel(1);
    let backend = backend(sender);
    let owner = std::thread::spawn(move || {
        drop(receiver.recv());
    });
    assert_eq!(backend.command(request()), Err(DesktopError::Unavailable));
    owner.join().map_err(|_| "owner panicked")?;
    Ok(())
}

#[test]
fn preference_save_persists_and_retries_only_the_latest_failed_event() -> Result<(), String> {
    use crate::settings::{AppearanceThemePreference, UiLocale};
    let directory = tempfile::tempdir().map_err(|_| "temporary directory unavailable")?;
    let (sender, _receiver) = mpsc::sync_channel(1);
    let mut backend = backend(sender);
    backend.preferences_path = Some(directory.path().join("ui-preferences.json"));
    backend
        .save_preferences(PreferencesPatch {
            theme: None,
            locale: Some(UiLocale::Russian),
        })
        .map_err(|_| "locale save failed")?;
    assert_eq!(
        backend.flush_preferences_update(|_| Err(DesktopError::Unavailable)),
        Err(DesktopError::Unavailable)
    );
    backend
        .save_preferences(PreferencesPatch {
            theme: Some(AppearanceThemePreference::Dark),
            locale: None,
        })
        .map_err(|_| "theme save failed")?;
    let saved = backend
        .preferences()
        .map_err(|_| "preferences unavailable")?;
    assert_eq!(saved.locale, UiLocale::Russian);
    assert_eq!(saved.theme, AppearanceThemePreference::Dark);
    let mut published = Vec::new();
    backend
        .flush_preferences_update(|value| {
            published.push(value);
            Ok(())
        })
        .map_err(|_| "publication failed")?;
    backend
        .flush_preferences_update(|_| Err(DesktopError::Rejected))
        .map_err(|_| "update was replayed")?;
    assert_eq!(published, vec![saved]);
    Ok(())
}

#[test]
fn failed_preference_save_neither_overwrites_file_nor_publishes_success() -> Result<(), String> {
    use crate::settings::UiLocale;
    let directory = tempfile::tempdir().map_err(|_| "temporary directory unavailable")?;
    let path = directory.path().join("ui-preferences.json");
    std::fs::write(&path, b"corrupt").map_err(|_| "fixture write failed")?;
    let (sender, _receiver) = mpsc::sync_channel(1);
    let mut backend = backend(sender);
    backend.preferences_path = Some(path.clone());
    assert_eq!(
        backend.save_preferences(PreferencesPatch {
            theme: None,
            locale: Some(UiLocale::Russian)
        }),
        Err(DesktopError::Unavailable)
    );
    backend
        .flush_preferences_update(|_| Err(DesktopError::Rejected))
        .map_err(|_| "false publication")?;
    assert_eq!(
        std::fs::read(path).map_err(|_| "fixture read failed")?,
        b"corrupt"
    );
    Ok(())
}

#[test]
fn settings_request_returns_owner_rejection_over_the_existing_bounded_queue() -> Result<(), String>
{
    use crate::tray::desktop::settings::SettingsRequest;
    let (sender, receiver) = mpsc::sync_channel(1);
    let backend = std::sync::Arc::new(backend(sender));
    let caller = backend.clone();
    let join = std::thread::spawn(move || caller.settings(SettingsRequest::Open {}));
    let WorkerCommand::DesktopSettings {
        request: actual,
        result_sender,
    } = receiver.recv().map_err(|_| "missing request")?
    else {
        return Err("wrong queue command".into());
    };
    assert_eq!(actual, SettingsRequest::Open {});
    assert_eq!(backend.command(request()), Err(DesktopError::Busy));
    result_sender
        .send(Err(DesktopError::Rejected))
        .map_err(|_| "missing caller")?;
    assert_eq!(
        join.join().map_err(|_| "caller failed")?,
        Err(DesktopError::Rejected)
    );
    Ok(())
}

#[test]
fn capture_readiness_requires_a_folder_and_preserves_active_safety_controls() -> Result<(), String>
{
    use crate::tray::desktop::model::{CaptureCommand, CaptureKind, CapturePhase};
    let (sender, _receiver) = mpsc::sync_channel(1);
    let mut backend = backend(sender);
    let unconfigured = backend.snapshot().map_err(|_| "snapshot unavailable")?;
    assert!(unconfigured.setup_required);
    assert!(!unconfigured.commands.contains(&CaptureCommand::Start));
    assert!(!unconfigured.capabilities.screenshot.source_available);
    let directory = tempfile::tempdir().map_err(|_| "temporary directory unavailable")?;
    let draft = crate::settings::LocalProfileDraft::manual(
        crate::settings::ManualProfileDraft::with_defaults(
            "destination".to_owned(),
            directory.path().to_string_lossy().into_owned(),
        ),
    );
    let profile = crate::settings::apply_local_profile_draft_from_path(
        Some(directory.path().join("local-profile.json")),
        &backend.settings,
        &draft,
    )
    .map_err(|_| "profile creation failed")?;
    backend.settings = ActiveSettings::from_profile(&profile);
    let configured = backend
        .snapshot()
        .map_err(|_| "configured snapshot unavailable")?;
    assert!(!configured.setup_required);
    assert!(configured.revision > unconfigured.revision);
    backend.projection.begin(
        "recording",
        None,
        CaptureKind::Video,
        CapturePhase::Starting,
        Some("recording-id"),
    );
    backend
        .projection
        .recording("recording-id", CapturePhase::Recording, 0);
    backend.operational = false;
    let recording = backend
        .snapshot()
        .map_err(|_| "recording snapshot unavailable")?;
    assert!(recording.commands.contains(&CaptureCommand::Stop));
    assert!(recording.commands.contains(&CaptureCommand::Discard));
    Ok(())
}

#[test]
fn window_availability_is_an_owner_fact_not_platform_or_recording_identity() -> Result<(), String> {
    let (sender, _receiver) = mpsc::sync_channel(1);
    let backend = backend(sender);
    let directory = tempfile::tempdir().map_err(|_| "temporary directory unavailable")?;
    let draft = crate::settings::LocalProfileDraft::manual(
        crate::settings::ManualProfileDraft::with_defaults(
            "destination".to_owned(),
            directory.path().to_string_lossy().into_owned(),
        ),
    );
    crate::settings::apply_local_profile_draft_from_path(
        Some(directory.path().join("profile.json")),
        &backend.settings,
        &draft,
    )
    .map_err(|_| "profile creation failed")?;
    let before = backend.snapshot().map_err(|_| "snapshot unavailable")?;
    assert!(!before.capabilities.targets.window.source_available);
    backend.observe_window_available(true);
    let ready = backend.snapshot().map_err(|_| "snapshot unavailable")?;
    assert!(ready.capabilities.targets.window.source_available);
    assert_eq!(ready.operation_epoch, before.operation_epoch);
    assert!(ready.revision > before.revision);
    backend.observe_window_available(false);
    let lost = backend.snapshot().map_err(|_| "snapshot unavailable")?;
    assert!(!lost.capabilities.targets.window.source_available);
    assert!(lost.revision > ready.revision);
    Ok(())
}
