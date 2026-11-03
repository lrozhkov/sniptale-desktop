use super::*;

#[test]
fn presentation_view_preserves_each_manual_video_and_audio_value() {
    let mut draft =
        ManualProfileDraft::with_defaults("destination".to_owned(), "C:\\Capture".to_owned());
    draft.set_video(
        ManualVideoQuality::Ultra,
        ManualFps::Fps60,
        Some(32),
        ManualHardwarePreference::ForceSoftware,
        false,
        900,
    );
    draft.set_audio(
        ManualAudioSource::Microphone,
        Some("microphone-1".to_owned()),
        192,
    );
    assert!(
        draft
            .set_hotkey(
                crate::settings::ManualHotkeyAction::ScreenshotScreen,
                crate::settings::parse_manual_hotkey_chord("Ctrl+Shift+S"),
            )
            .is_ok()
    );

    let view = draft.view();

    assert_eq!(view.video_quality, ManualVideoQuality::Ultra);
    assert_eq!(view.video_fps, ManualFps::Fps60);
    assert_eq!(view.video_bitrate_override_mbps, Some(32));
    assert_eq!(
        view.video_hardware_preference,
        ManualHardwarePreference::ForceSoftware
    );
    assert!(!view.video_include_cursor);
    assert_eq!(view.video_max_duration_seconds, 900);
    assert_eq!(view.audio_source_mode, ManualAudioSource::Microphone);
    assert_eq!(view.microphone_device_id.as_deref(), Some("microphone-1"));
    assert_eq!(view.audio_bitrate_kbps, 192);
    assert_eq!(
        view.hotkeys.get("screenshot-screen"),
        Some(&"Ctrl+Shift+S".to_owned())
    );
}

#[test]
fn production_mutators_update_cursor_and_microphone_fields() {
    let mut draft =
        ManualProfileDraft::with_defaults("destination".to_owned(), "C:\\Capture".to_owned());
    draft.set_video_include_cursor(false);
    draft.set_microphone_device_id(Some("microphone-1".to_owned()));

    let view = draft.view();
    assert!(!view.video_include_cursor);
    assert_eq!(view.microphone_device_id.as_deref(), Some("microphone-1"));
}

#[test]
fn web_draft_rejects_invalid_bounds_paths_hotkeys_and_extra_fields()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let valid = super::ManualProfileDraft::with_defaults(
        "retained-id".to_owned(),
        directory.path().to_string_lossy().into_owned(),
    )
    .view();
    let raw = serde_json::to_value(&valid)?;
    let mut foreign = raw.clone();
    foreign["unknown"] = serde_json::json!(true);
    assert!(serde_json::from_value::<super::ManualProfileDraftView>(foreign).is_err());
    for (field, value) in [
        ("destinationPath", serde_json::json!("../outside")),
        ("videoMaxDurationSeconds", serde_json::json!(0)),
        ("videoBitrateOverrideMbps", serde_json::json!(81)),
        ("audioBitrateKbps", serde_json::json!(32)),
        ("microphoneDeviceId", serde_json::json!("x".repeat(129))),
        ("hotkeys", serde_json::json!({"unknown-action":"Ctrl+S"})),
        (
            "hotkeys",
            serde_json::json!({"screenshot-region":"Ctrl+S", "recording-stop":"Ctrl+S"}),
        ),
    ] {
        let mut changed = raw.clone();
        changed[field] = value;
        let view = serde_json::from_value(changed)?;
        assert!(
            super::ManualProfileDraft::from_view(view, "retained-id".to_owned()).is_err(),
            "{field}"
        );
    }
    let draft = super::ManualProfileDraft::from_view(valid.clone(), "retained-id".to_owned())
        .map_err(|error| format!("{error:?}"))?;
    assert_eq!(draft.view(), valid);
    assert_eq!(draft.into_profile().destination.id, "retained-id");
    Ok(())
}
