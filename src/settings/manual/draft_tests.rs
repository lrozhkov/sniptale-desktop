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
