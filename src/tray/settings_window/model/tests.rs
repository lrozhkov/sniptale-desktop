use tempfile::tempdir;

use super::{
    CloseDisposition, SettingsWindowFeedback, SettingsWindowModel, SettingsWindowModelError,
    SettingsWindowSection, SettingsWindowView,
};
use crate::settings::{
    ActiveSettings, LocalProfileDraft, ManualProfileDraft, ensure_local_profile_from_path,
};

#[test]
fn extension_view_is_read_only_until_manual_setup_is_staged()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempdir()?;
    let profile = ensure_local_profile_from_path(Some(directory.path().join("profile.json")))
        .map_err(|error| std::io::Error::other(format!("{error:?}")))?;
    let active = ActiveSettings::from_profile(&profile);
    let mut model = SettingsWindowModel::default();
    assert_eq!(
        model
            .show(&active, crate::settings::UiPreferencesHealth::Missing)
            .map_err(debug_error)?,
        SettingsWindowView::ExtensionManagedReadOnly
    );
    assert_eq!(
        model.apply_candidate(),
        Err(SettingsWindowModelError::NoChanges)
    );
    model.stage_manual(LocalProfileDraft::manual(
        ManualProfileDraft::with_defaults(
            "destination".to_owned(),
            directory.path().to_string_lossy().into_owned(),
        ),
    ));
    model.refresh_audio_capabilities(available_audio(Vec::new()));
    assert_eq!(model.view(), SettingsWindowView::ManualDraft);
    assert!(model.apply_candidate().is_ok());
    Ok(())
}

#[test]
fn cancel_discards_the_candidate() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempdir()?;
    let profile = ensure_local_profile_from_path(Some(directory.path().join("profile.json")))
        .map_err(debug_error)?;
    let active = ActiveSettings::from_profile(&profile);
    let mut model = SettingsWindowModel::default();
    let _ = model
        .show(&active, crate::settings::UiPreferencesHealth::Missing)
        .map_err(debug_error)?;
    model.stage_manual(LocalProfileDraft::manual(
        ManualProfileDraft::with_defaults(
            "destination".to_owned(),
            directory.path().to_string_lossy().into_owned(),
        ),
    ));
    model.discard_and_hide();
    assert_eq!(model.view(), SettingsWindowView::ExtensionManagedReadOnly);
    assert_eq!(
        model.apply_candidate(),
        Err(SettingsWindowModelError::NoChanges)
    );
    Ok(())
}

#[test]
fn mode_staging_and_manual_presentation_are_complete() -> Result<(), std::io::Error> {
    let mut model = SettingsWindowModel::default();
    model.stage_manual_destination("destination".to_owned(), "C:\\Capture".to_owned());
    model.set_screenshot_include_cursor(false);
    let view = model
        .manual_view()
        .ok_or_else(|| std::io::Error::other("view"))?;
    assert!(!view.screenshot_include_cursor);
    assert!(view.video_include_cursor);
    assert_eq!(view.audio_bitrate_kbps, 128);
    assert_eq!(model.presentation().manual(), Some(&view));
    model.stage_extension_managed();
    assert_eq!(model.view(), SettingsWindowView::ExtensionManagedDraft);
    Ok(())
}

#[test]
fn microphone_presentation_uses_labels_but_selects_stable_ids() {
    let mut model = SettingsWindowModel::default();
    model.stage_manual_destination("destination".to_owned(), "C:\\Capture".to_owned());
    model.refresh_audio_capabilities(available_audio(vec![
        crate::audio::MicrophoneDeviceDescriptor {
            id: "private-device-id".to_owned(),
            label: "Studio microphone".to_owned(),
            is_default: true,
            available: true,
        },
    ]));
    model.select_microphone(super::SettingsMicrophoneSelection::Device(
        "private-device-id".to_owned(),
    ));
    let presentation = model.presentation();
    assert_eq!(
        presentation.microphones()[1].label(),
        "Studio microphone (default)"
    );
    assert_eq!(presentation.selected_microphone_index(), 1);
    assert_eq!(
        model
            .manual_view()
            .and_then(|manual| manual.microphone_device_id),
        Some("private-device-id".to_owned())
    );
}

#[test]
fn dirty_close_requires_explicit_discard_and_keep_editing_preserves_draft() {
    let mut model = SettingsWindowModel::default();
    model.stage_manual_destination("destination".to_owned(), "C:\\Capture".to_owned());
    model.refresh_audio_capabilities(available_audio(Vec::new()));
    assert!(model.dirty());
    assert!(model.presentation().can_apply());
    assert_eq!(model.request_close(), CloseDisposition::ConfirmDiscard);
    assert!(model.presentation().confirm_discard());
    model.keep_editing();
    assert!(!model.presentation().confirm_discard());
    assert!(model.manual_view().is_some());
    model.discard_and_hide();
    assert!(!model.dirty());
}

#[test]
fn unavailable_audio_sources_and_devices_block_apply_without_losing_selection() {
    let mut model = SettingsWindowModel::default();
    model.stage_manual_destination("destination".to_owned(), "C:\\Capture".to_owned());
    model.refresh_audio_capabilities(crate::audio::AudioCapabilitySummary {
        microphone_devices: Vec::new(),
        supports_microphone: false,
        supports_system_audio: true,
        supports_mixed_audio: false,
        unavailable_reasons: Vec::new(),
    });
    model.set_audio_source(crate::settings::ManualAudioSource::System);
    assert!(model.presentation().can_apply());
    model.set_audio_source(crate::settings::ManualAudioSource::Mixed);
    assert_eq!(
        model.presentation().feedback(),
        Some(SettingsWindowFeedback::AudioSourceUnavailable)
    );
    assert_eq!(
        model.manual_view().map(|manual| manual.audio_source_mode),
        Some(crate::settings::ManualAudioSource::System)
    );
}

#[test]
fn appearance_and_inline_feedback_are_projected_without_dirtying_capture_settings() {
    let mut model = SettingsWindowModel::default();
    model.appearance_saved(crate::settings::AppearanceThemePreference::Dark);
    assert_eq!(
        model.presentation().theme(),
        crate::settings::AppearanceThemePreference::Dark
    );
    assert!(!model.presentation().dirty());
    model.appearance_save_failed();
    assert_eq!(
        model.presentation().feedback(),
        Some(SettingsWindowFeedback::AppearanceSaveFailed)
    );
    for feedback in [
        SettingsWindowFeedback::SaveFailed,
        SettingsWindowFeedback::RecordingBusy,
        SettingsWindowFeedback::HotkeyUnavailable,
    ] {
        model.set_apply_feedback(feedback);
        assert_eq!(model.presentation().feedback(), Some(feedback));
    }
}

#[test]
fn navigation_is_presentation_only_and_all_sections_have_copy() {
    let mut model = SettingsWindowModel::default();
    for section in SettingsWindowSection::ALL {
        model.navigate(section);
        assert_eq!(model.presentation().section(), section);
        assert!(!section.label().is_empty());
    }
}

fn debug_error(error: impl std::fmt::Debug) -> std::io::Error {
    std::io::Error::other(format!("{error:?}"))
}

fn available_audio(
    microphone_devices: Vec<crate::audio::MicrophoneDeviceDescriptor>,
) -> crate::audio::AudioCapabilitySummary {
    crate::audio::AudioCapabilitySummary {
        microphone_devices,
        supports_microphone: true,
        supports_system_audio: true,
        supports_mixed_audio: true,
        unavailable_reasons: Vec::new(),
    }
}
