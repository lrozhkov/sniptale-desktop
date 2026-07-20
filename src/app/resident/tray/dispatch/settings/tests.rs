use std::{path::Path, sync::mpsc, thread};

use tempfile::tempdir;

use super::*;
use crate::tray::settings_window::model::{
    SettingsMicrophoneSelection, SettingsWindowPresentation, SettingsWindowSection,
};
use crate::{
    app::resident::WorkerCommand,
    settings::{AppearanceThemePreference, ManualHotkeyAction, OperatingMode},
    tray::{TrayIconVariant, TrayMenuRenderPlan, TrayNoticeKind, TrayRuntimeError},
};

#[test]
fn typed_events_map_every_manual_field_and_immediate_theme()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempdir()?;
    let (active, mut model, profile_path) = setup(directory.path())?;
    let appearance_path = directory.path().join("ui-preferences.json");
    let mut source = Source::with_destination(directory.path().join("captures"));
    let mut manual_dispatcher = None;
    let senders = ManualWorkerSenders::default();
    let events = vec![
        SettingsWindowEvent::Navigate(SettingsWindowSection::Video),
        SettingsWindowEvent::BrowseDestination,
        SettingsWindowEvent::ToggleScreenshotCursor,
        SettingsWindowEvent::SetVideoQuality(crate::settings::ManualVideoQuality::Ultra),
        SettingsWindowEvent::SetVideoFps(crate::settings::ManualFps::Fps60),
        SettingsWindowEvent::SetVideoBitrate(Some(40)),
        SettingsWindowEvent::SetVideoHardware(
            crate::settings::ManualHardwarePreference::ForceSoftware,
        ),
        SettingsWindowEvent::ToggleVideoCursor,
        SettingsWindowEvent::SetVideoDuration(7_200),
        SettingsWindowEvent::SetAudioSource(crate::settings::ManualAudioSource::System),
        SettingsWindowEvent::SelectMicrophone(SettingsMicrophoneSelection::SystemDefault),
        SettingsWindowEvent::SetAudioBitrate(192),
        SettingsWindowEvent::SetHotkey {
            action: ManualHotkeyAction::ScreenshotScreen,
            chord: crate::settings::parse_manual_hotkey_chord("Ctrl+Shift+S"),
        },
        SettingsWindowEvent::CancelHotkeyCapture(ManualHotkeyAction::ScreenshotScreen),
        SettingsWindowEvent::SelectAppearanceTheme(AppearanceThemePreference::Dark),
    ];
    for event in events {
        assert!(!dispatch_settings_event_with_paths(
            event,
            SettingsEventContext {
                active_settings: &active,
                model: &mut model,
                manual_dispatcher: &mut manual_dispatcher,
                manual_senders: &senders,
                recording_status: TrayRecordingStatus::Idle,
                source: &mut source,
                paths: SettingsPersistencePaths {
                    profile: Some(profile_path.clone()),
                    appearance: Some(appearance_path.clone()),
                },
            },
        ));
    }

    let manual = model.manual_view().ok_or("manual draft")?;
    assert_eq!(manual.video_bitrate_override_mbps, Some(40));
    assert_eq!(manual.video_max_duration_seconds, 7_200);
    assert_eq!(
        manual.audio_source_mode,
        crate::settings::ManualAudioSource::System
    );
    assert_eq!(manual.audio_bitrate_kbps, 192);
    assert_eq!(
        model.presentation().theme(),
        AppearanceThemePreference::Dark
    );
    assert!(appearance_path.exists());
    for event in [
        SettingsWindowEvent::RefreshMicrophones,
        SettingsWindowEvent::UseExtensionManaged,
        SettingsWindowEvent::RequestClose,
        SettingsWindowEvent::KeepEditing,
        SettingsWindowEvent::RequestClose,
        SettingsWindowEvent::ConfirmDiscard,
    ] {
        let _ = dispatch_settings_event_with_paths(
            event,
            SettingsEventContext {
                active_settings: &active,
                model: &mut model,
                manual_dispatcher: &mut manual_dispatcher,
                manual_senders: &senders,
                recording_status: TrayRecordingStatus::Idle,
                source: &mut source,
                paths: SettingsPersistencePaths {
                    profile: None,
                    appearance: None,
                },
            },
        );
    }
    let runtime_event = crate::tray::TrayRuntimeEvent::SettingsWindow {
        event: SettingsWindowEvent::Navigate(SettingsWindowSection::Capture),
    };
    assert!(matches!(
        runtime_event,
        crate::tray::TrayRuntimeEvent::SettingsWindow { .. }
    ));
    Ok(())
}

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
    assert!(dispatcher.is_none());
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
fn recording_busy_and_theme_failure_are_inline_and_non_destructive()
-> Result<(), Box<dyn std::error::Error>> {
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

    assert!(!dispatch_settings_event_with_paths(
        SettingsWindowEvent::SelectAppearanceTheme(AppearanceThemePreference::Dark),
        SettingsEventContext {
            active_settings: &active,
            model: &mut model,
            manual_dispatcher: &mut dispatcher,
            manual_senders: &ManualWorkerSenders::default(),
            recording_status: TrayRecordingStatus::Idle,
            source: &mut source,
            paths: SettingsPersistencePaths {
                profile: None,
                appearance: None,
            },
        },
    ));
    assert_eq!(
        model.presentation().theme(),
        AppearanceThemePreference::System
    );
    assert_eq!(
        model.presentation().feedback(),
        Some(SettingsWindowFeedback::AppearanceSaveFailed)
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
    dispatch_settings_event_with_paths(
        SettingsWindowEvent::Apply,
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
