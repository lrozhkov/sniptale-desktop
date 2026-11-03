use std::{path::Path, sync::mpsc, thread};

use tempfile::tempdir;

use super::*;
use crate::tray::desktop::{model::DesktopError, settings::SettingsRequest};
use crate::tray::settings_window::model::{
    CloseDisposition, SettingsWindowPresentation, SettingsWindowSection,
};
use crate::{
    app::resident::WorkerCommand,
    settings::{ManualHotkeyAction, OperatingMode},
    tray::{TrayIconVariant, TrayMenuRenderPlan, TrayNoticeKind, TrayRuntimeError},
};

#[test]
fn apply_covers_manual_and_extension_mode_flows() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempdir()?;
    let (active, mut model, profile_path) = setup(directory.path())?;
    let mut source = Source::default();
    let mut dispatcher = None;
    let senders = ManualWorkerSenders::default();
    assert!(dispatch_apply(
        &active,
        &mut model,
        &mut dispatcher,
        &senders,
        &mut source,
        profile_path.clone(),
        TrayRecordingStatus::Idle,
    ));
    assert_eq!(
        active.profile_snapshot()?.operating_mode(),
        OperatingMode::Manual
    );
    assert!(dispatcher.is_some());

    let _ = model.show(&active, crate::settings::UiPreferencesHealth::Missing);
    model.refresh_audio_capabilities(available_audio());
    model.stage_extension_managed();
    assert!(dispatch_apply(
        &active,
        &mut model,
        &mut dispatcher,
        &senders,
        &mut source,
        profile_path,
        TrayRecordingStatus::Idle,
    ));
    assert_eq!(
        active.profile_snapshot()?.operating_mode(),
        OperatingMode::ExtensionManaged
    );
    assert!(dispatcher.is_some());
    Ok(())
}

#[test]
fn rejected_hotkey_stage_restores_old_authority_and_profile()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempdir()?;
    let (active, mut model, profile_path) = setup(directory.path())?;
    model.set_hotkey(
        ManualHotkeyAction::ScreenshotScreen,
        crate::settings::parse_manual_hotkey_chord("Ctrl+Shift+S"),
    );
    let original = active.profile_snapshot()?;
    let (sender, receiver) = mpsc::sync_channel(1);
    let worker = thread::spawn(move || {
        if let Ok(WorkerCommand::FenceMode { result_sender }) = receiver.recv() {
            let _ = result_sender.send(false);
        }
        if let Ok(WorkerCommand::ApplyHotkeyPlan { result_sender, .. }) = receiver.recv() {
            let _ = result_sender.send(Err(crate::hotkeys::HotkeyRuntimeError::RegisterFailed {
                registration_id: 1,
            }));
        }
        if let Ok(WorkerCommand::RestoreHotkeys { result_sender }) = receiver.recv() {
            let _ = result_sender.send(Ok(()));
        }
    });
    let senders = ManualWorkerSenders::new(None, None, None, Some(sender), None);
    let mut source = Source::default();
    let mut dispatcher = None;

    assert!(!dispatch_apply(
        &active,
        &mut model,
        &mut dispatcher,
        &senders,
        &mut source,
        profile_path.clone(),
        TrayRecordingStatus::Idle,
    ));
    worker.join().map_err(|_| "hotkey worker")?;
    assert_eq!(
        crate::settings::ensure_local_profile_from_path(Some(profile_path))
            .map_err(|error| format!("{error:?}"))?,
        original
    );
    assert_eq!(
        active.profile_snapshot()?.operating_mode(),
        OperatingMode::ExtensionManaged
    );
    assert_eq!(
        model.presentation().feedback(),
        Some(SettingsWindowFeedback::HotkeyUnavailable)
    );
    Ok(())
}

#[test]
fn recording_busy_is_inline_and_preserves_the_profile() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempdir()?;
    let (active, mut model, profile_path) = setup(directory.path())?;
    let original = active.profile_snapshot()?;
    let mut source = Source::default();
    let mut dispatcher = None;
    assert!(!dispatch_apply(
        &active,
        &mut model,
        &mut dispatcher,
        &ManualWorkerSenders::default(),
        &mut source,
        profile_path.clone(),
        TrayRecordingStatus::Recording,
    ));
    assert_eq!(
        crate::settings::ensure_local_profile_from_path(Some(profile_path))
            .map_err(|error| format!("{error:?}"))?,
        original
    );
    assert_eq!(
        model.presentation().feedback(),
        Some(SettingsWindowFeedback::RecordingBusy)
    );

    Ok(())
}

fn dispatch_apply(
    active: &crate::settings::ActiveSettings,
    model: &mut SettingsWindowModel,
    dispatcher: &mut Option<ManualActionDispatcher>,
    senders: &ManualWorkerSenders,
    source: &mut Source,
    profile: std::path::PathBuf,
    status: TrayRecordingStatus,
) -> bool {
    apply_settings_draft(
        SettingsEventContext {
            active_settings: active,
            model,
            manual_dispatcher: dispatcher,
            manual_senders: senders,
            recording_status: status,
            source,
            paths: SettingsPersistencePaths {
                profile: Some(profile),
                appearance: None,
            },
        },
        false,
    )
}

fn setup(
    root: &Path,
) -> Result<
    (
        crate::settings::ActiveSettings,
        SettingsWindowModel,
        std::path::PathBuf,
    ),
    Box<dyn std::error::Error>,
> {
    let profile_path = root.join("local-profile.json");
    let profile = crate::settings::ensure_local_profile_from_path(Some(profile_path.clone()))
        .map_err(|error| format!("{error:?}"))?;
    let active = crate::settings::ActiveSettings::from_profile(&profile);
    let mut model = SettingsWindowModel::default();
    let _ = model.show(&active, crate::settings::UiPreferencesHealth::Missing);
    model.stage_manual_destination(
        "destination".to_owned(),
        root.to_string_lossy().into_owned(),
    );
    model.refresh_audio_capabilities(available_audio());
    Ok((active, model, profile_path))
}

fn available_audio() -> crate::audio::AudioCapabilitySummary {
    crate::audio::AudioCapabilitySummary {
        microphone_devices: Vec::new(),
        supports_microphone: true,
        supports_system_audio: true,
        supports_mixed_audio: true,
        unavailable_reasons: Vec::new(),
    }
}

#[derive(Default)]
struct Source {
    destination: Option<std::path::PathBuf>,
    presentations: Vec<SettingsWindowPresentation>,
    hidden: usize,
}

impl Source {
    fn with_destination(destination: std::path::PathBuf) -> Self {
        Self {
            destination: Some(destination),
            ..Self::default()
        }
    }
}

impl TrayCommandSource for Source {
    fn apply_render_plan(
        &mut self,
        _plan: &TrayMenuRenderPlan,
        _tooltip: &str,
        _icon: TrayIconVariant,
    ) -> Result<(), TrayRuntimeError> {
        Ok(())
    }

    fn drain_events(&mut self) -> Result<Vec<crate::tray::TrayRuntimeEvent>, TrayRuntimeError> {
        Ok(Vec::new())
    }

    fn show_notice(&mut self, _kind: TrayNoticeKind) -> Result<(), TrayRuntimeError> {
        Ok(())
    }

    fn show_settings_window(
        &mut self,
        presentation: SettingsWindowPresentation,
    ) -> Result<(), TrayRuntimeError> {
        self.presentations.push(presentation);
        Ok(())
    }

    fn hide_settings_window(&mut self) -> Result<(), TrayRuntimeError> {
        self.hidden += 1;
        Ok(())
    }

    fn pick_settings_destination(
        &mut self,
    ) -> Result<Option<std::path::PathBuf>, TrayRuntimeError> {
        Ok(self.destination.take())
    }

    fn open_save_folder(&mut self, _destination: &Path) -> Result<(), TrayRuntimeError> {
        Ok(())
    }
}

#[test]
fn browser_choice_is_explicit_persisted_and_blocked_while_recording()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempdir()?;
    let (active, mut model, profile_path) = setup(directory.path())?;
    let path = directory.path().join("browser-targets.json");
    let active = active
        .with_browser_target_store(path.clone())
        .map_err(|_| "registry")?;
    let identity = crate::protocol::NativeControllerIdentity {
        extension_id: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned(),
        browser_family: crate::protocol::BrowserFamily::Chrome,
        profile_key: "profile-one".to_owned(),
        document_id: None,
        connection_id: "connection-one".to_owned(),
    };
    let target = active
        .register_browser_target(&identity)
        .map_err(|_| "register")?;
    let mut source = Source::default();
    let mut dispatcher = None;
    let senders = ManualWorkerSenders::default();
    for (status, expected) in [
        (
            TrayRecordingStatus::Recording,
            Some(DesktopError::RecordingBusy),
        ),
        (TrayRecordingStatus::Idle, None),
    ] {
        let result = dispatch_desktop_settings_with_context(
            SettingsRequest::SaveDelivery {
                target_id: Some(target.id.clone()),
                ask_after_capture: true,
            },
            SettingsEventContext {
                active_settings: &active,
                model: &mut model,
                manual_dispatcher: &mut dispatcher,
                manual_senders: &senders,
                recording_status: status,
                source: &mut source,
                paths: SettingsPersistencePaths {
                    profile: Some(profile_path.clone()),
                    appearance: None,
                },
            },
            |model| model.refresh_audio_capabilities(available_audio()),
        );
        if let Some(error) = expected {
            assert_eq!(result, Err(error));
            assert_eq!(
                active
                    .browser_targets()
                    .map_err(|_| "registry")?
                    .default_target_id(),
                None
            );
        } else {
            let saved = result.map_err(|error| format!("{error:?}"))?;
            assert_eq!(
                saved.delivery.target_id.as_deref(),
                Some(target.id.as_str())
            );
            assert!(saved.delivery.ask_after_capture);
        }
    }
    let reloaded = crate::settings::BrowserTargetRegistry::load(path).map_err(|_| "reload")?;
    assert_eq!(reloaded.default_target_id(), Some(target.id.as_str()));
    assert!(reloaded.ask_after_capture());
    Ok(())
}

#[test]
fn repeated_open_preserves_custom_draft_section_and_dirty_close()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempdir()?;
    let (active, mut model, _) = setup(directory.path())?;
    model.navigate(SettingsWindowSection::Video);
    model.set_video_bitrate(Some(37));
    let before = model.presentation();
    let mut source = Source::default();
    crate::app::resident::tray::dispatch::open_settings(&active, &mut model, &mut source);
    assert_eq!(source.presentations.last(), Some(&before));
    assert_eq!(model.presentation(), before);
    assert_eq!(model.request_close(), CloseDisposition::ConfirmDiscard);
    Ok(())
}

#[test]
fn desktop_apply_keeps_window_open_and_rejects_stale_invalid_and_busy_submissions()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempdir()?;
    let (active, mut model, profile_path) = setup(directory.path())?;
    let mut source = Source::default();
    let mut dispatcher = None;
    let senders = ManualWorkerSenders::default();
    let mut draft = model.manual_view().ok_or("manual draft")?;
    draft.video_quality = crate::settings::ManualVideoQuality::Ultra;
    let revision = active.profile_snapshot()?.revision();
    let paths = SettingsPersistencePaths {
        profile: Some(profile_path.clone()),
        appearance: None,
    };
    let saved = dispatch_desktop_settings_with_context(
        SettingsRequest::Apply {
            expected_revision: revision,
            draft: draft.clone(),
        },
        SettingsEventContext {
            active_settings: &active,
            model: &mut model,
            manual_dispatcher: &mut dispatcher,
            manual_senders: &senders,
            recording_status: TrayRecordingStatus::Idle,
            source: &mut source,
            paths: paths.clone(),
        },
        |model| model.refresh_audio_capabilities(available_audio()),
    )
    .map_err(|error| format!("{error:?}"))?;
    assert!(saved.profile_revision > revision);
    assert_eq!(saved.draft, draft);
    assert!(model.visible());
    assert_eq!(source.hidden, 0);
    let stored = fs_profile(&profile_path)?;
    for (expected_revision, duration, status, error) in [
        (
            revision,
            120,
            TrayRecordingStatus::Idle,
            DesktopError::StaleSettings,
        ),
        (
            saved.profile_revision,
            0,
            TrayRecordingStatus::Idle,
            DesktopError::InvalidSettings,
        ),
        (
            saved.profile_revision,
            120,
            TrayRecordingStatus::Recording,
            DesktopError::RecordingBusy,
        ),
    ] {
        let mut changed = draft.clone();
        changed.video_max_duration_seconds = duration;
        assert_eq!(
            dispatch_desktop_settings_with_context(
                SettingsRequest::Apply {
                    expected_revision,
                    draft: changed
                },
                SettingsEventContext {
                    active_settings: &active,
                    model: &mut model,
                    manual_dispatcher: &mut dispatcher,
                    manual_senders: &senders,
                    recording_status: status,
                    source: &mut source,
                    paths: paths.clone()
                },
                |model| model.refresh_audio_capabilities(available_audio()),
            ),
            Err(error)
        );
        assert_eq!(fs_profile(&profile_path)?, stored);
        assert_eq!(
            active.profile_snapshot()?.revision(),
            saved.profile_revision
        );
    }
    assert_eq!(
        model
            .manual_view()
            .ok_or("retained draft")?
            .video_max_duration_seconds,
        120
    );
    Ok(())
}

fn fs_profile(path: &Path) -> Result<Vec<u8>, std::io::Error> {
    std::fs::read(path)
}

#[test]
fn desktop_folder_cancel_is_distinct_from_an_earlier_selection()
-> Result<(), Box<dyn std::error::Error>> {
    use crate::tray::desktop::settings::SettingsRequest;
    let directory = tempdir()?;
    let (active, mut model, profile_path) = setup(directory.path())?;
    let destination = directory.path().join("chosen");
    let mut source = Source::with_destination(destination.clone());
    let mut dispatcher = None;
    let senders = ManualWorkerSenders::default();
    for expected in [Some(destination.to_string_lossy().into_owned()), None] {
        let snapshot = dispatch_desktop_settings_with_context(
            SettingsRequest::ChooseFolder {},
            SettingsEventContext {
                active_settings: &active,
                model: &mut model,
                manual_dispatcher: &mut dispatcher,
                manual_senders: &senders,
                recording_status: TrayRecordingStatus::Idle,
                source: &mut source,
                paths: SettingsPersistencePaths {
                    profile: Some(profile_path.clone()),
                    appearance: None,
                },
            },
            |model| model.refresh_audio_capabilities(available_audio()),
        )
        .map_err(|error| format!("{error:?}"))?;
        assert_eq!(snapshot.selected_folder, expected);
    }
    Ok(())
}

#[test]
fn desktop_browser_delivery_uses_stable_ids_and_preserves_profile_and_refused_choices()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempdir()?;
    let (active, mut model, profile_path) = setup(directory.path())?;
    let registry_path = directory.path().join("browser-targets.json");
    let active = active
        .with_browser_target_store(registry_path.clone())
        .map_err(|_| "registry")?;
    let target = active
        .register_browser_target(&crate::protocol::NativeControllerIdentity {
            extension_id: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned(),
            browser_family: crate::protocol::BrowserFamily::Chrome,
            profile_key: "personal".to_owned(),
            document_id: None,
            connection_id: "runtime-connection".to_owned(),
        })
        .map_err(|_| "target registration")?;
    let mut source = Source::default();
    let mut dispatcher = None;
    let senders = ManualWorkerSenders::default();
    let profile_before = fs_profile(&profile_path)?;
    for (target_id, status, expected) in [
        (
            Some(target.id.clone()),
            TrayRecordingStatus::Recording,
            Some(DesktopError::RecordingBusy),
        ),
        (
            Some("runtime-connection".to_owned()),
            TrayRecordingStatus::Idle,
            Some(DesktopError::StaleSettings),
        ),
        (Some(target.id.clone()), TrayRecordingStatus::Idle, None),
        (None, TrayRecordingStatus::Idle, None),
    ] {
        let registry_before = std::fs::read(&registry_path)?;
        let result = dispatch_desktop_settings_with_context(
            SettingsRequest::SaveDelivery {
                target_id: target_id.clone(),
                ask_after_capture: true,
            },
            SettingsEventContext {
                active_settings: &active,
                model: &mut model,
                manual_dispatcher: &mut dispatcher,
                manual_senders: &senders,
                recording_status: status,
                source: &mut source,
                paths: SettingsPersistencePaths {
                    profile: Some(profile_path.clone()),
                    appearance: None,
                },
            },
            |model| model.refresh_audio_capabilities(available_audio()),
        );
        if let Some(error) = expected {
            assert_eq!(result, Err(error));
            assert_eq!(std::fs::read(&registry_path)?, registry_before);
        } else {
            let snapshot = result.map_err(|error| format!("{error:?}"))?;
            assert_eq!(snapshot.delivery.target_id, target_id);
            assert!(snapshot.delivery.ask_after_capture);
            let persisted = crate::settings::BrowserTargetRegistry::load(registry_path.clone())
                .map_err(|_| "reload")?;
            assert_eq!(persisted.default_target_id(), target_id.as_deref());
        }
        assert_eq!(fs_profile(&profile_path)?, profile_before);
    }
    Ok(())
}
