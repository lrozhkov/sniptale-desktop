use super::*;

#[test]
fn presentation_distinguishes_mode_switches_from_same_mode_edits()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let profile_path = directory.path().join("profile.json");
    let profile = crate::settings::ensure_local_profile_from_path(Some(profile_path.clone()))
        .map_err(|error| std::io::Error::other(format!("{error:?}")))?;
    let active = ActiveSettings::from_profile(&profile);
    let mut model = SettingsWindowModel::default();
    let _ = model.show(&active, crate::settings::UiPreferencesHealth::Missing);
    assert_eq!(presentation_visibility(&model), (false, false, true));
    model.stage_manual_destination(
        "destination".to_owned(),
        directory.path().to_string_lossy().into_owned(),
    );
    model.refresh_audio_capabilities(crate::audio::AudioCapabilitySummary {
        microphone_devices: Vec::new(),
        supports_microphone: true,
        supports_system_audio: true,
        supports_mixed_audio: true,
        unavailable_reasons: Vec::new(),
    });
    assert!(model.presentation().switching_mode());
    assert_eq!(presentation_visibility(&model), (true, true, false));
    assert_eq!(
        model.presentation().apply_label(),
        "Save and switch to Manual"
    );

    let draft = model
        .apply_candidate()
        .map_err(|error| std::io::Error::other(format!("{error:?}")))?;
    crate::settings::apply_local_profile_draft_from_path(Some(profile_path), &active, &draft)
        .map_err(|error| std::io::Error::other(format!("{error:?}")))?;
    let _ = model.show(&active, crate::settings::UiPreferencesHealth::Missing);
    assert!(!model.presentation().switching_mode());
    assert_eq!(model.presentation().apply_label(), "Save changes");

    model.stage_extension_managed();
    assert!(model.presentation().switching_mode());
    assert_eq!(presentation_visibility(&model), (false, true, false));
    assert_eq!(model.presentation().apply_label(), "Switch to extension");
    model.discard_and_hide();
    assert!(!model.presentation().switching_mode());
    Ok(())
}

fn presentation_visibility(model: &SettingsWindowModel) -> (bool, bool, bool) {
    let presentation = model.presentation();
    (
        presentation.shows_navigation(),
        presentation.shows_footer(),
        presentation.shows_manual_setup(),
    )
}

#[test]
fn presentation_round_trips_custom_numeric_values_and_typed_audio_choices() {
    let mut model = SettingsWindowModel::default();
    model.stage_manual_destination("destination".to_owned(), "C:\\Capture".to_owned());
    model.refresh_audio_capabilities(crate::audio::AudioCapabilitySummary {
        microphone_devices: vec![crate::audio::MicrophoneDeviceDescriptor {
            id: "stable-device".to_owned(),
            label: "Microphone".to_owned(),
            is_default: true,
            available: true,
        }],
        supports_microphone: true,
        supports_system_audio: false,
        supports_mixed_audio: false,
        unavailable_reasons: Vec::new(),
    });
    model.set_video_bitrate(Some(40));
    model.set_video_duration(7_200);
    let presentation = model.presentation();
    let bitrate = presentation.video_bitrate_options();
    let duration = presentation.video_duration_options();

    assert!(
        bitrate
            .iter()
            .any(|option| option.value() == Some(40) && option.label() == "40 Mbps")
    );
    assert!(
        duration
            .iter()
            .any(|option| option.value() == 7_200 && option.label() == "2 hours")
    );
    assert_eq!(
        presentation.microphone_selection(1),
        Some(SettingsMicrophoneSelection::Device(
            "stable-device".to_owned()
        ))
    );
    assert_eq!(
        presentation.audio_source_selection(0),
        Some(crate::settings::ManualAudioSource::Microphone)
    );
    assert_eq!(presentation.audio_sources()[0].label(), "Microphone");
    assert!(presentation.audio_sources()[0].available());
}
