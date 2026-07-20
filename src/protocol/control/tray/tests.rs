use super::{ExtensionIntentActionId, NativeTrayActionKind};

#[test]
fn canonical_extension_intents_round_trip_and_require_their_matching_kind() {
    let cases = [
        (
            ExtensionIntentActionId::ScreenshotScreen,
            NativeTrayActionKind::CaptureScreenshot,
        ),
        (
            ExtensionIntentActionId::ScreenshotActiveWindow,
            NativeTrayActionKind::CaptureScreenshot,
        ),
        (
            ExtensionIntentActionId::ScreenshotAllScreens,
            NativeTrayActionKind::CaptureScreenshot,
        ),
        (
            ExtensionIntentActionId::ScreenshotRegion,
            NativeTrayActionKind::CaptureScreenshot,
        ),
        (
            ExtensionIntentActionId::RecordingStartScreen,
            NativeTrayActionKind::StartRecording,
        ),
        (
            ExtensionIntentActionId::RecordingStartActiveWindow,
            NativeTrayActionKind::StartRecording,
        ),
        (
            ExtensionIntentActionId::RecordingStartRegion,
            NativeTrayActionKind::StartRecording,
        ),
    ];
    for (intent, kind) in cases {
        assert_eq!(
            ExtensionIntentActionId::parse(intent.as_str()),
            Some(intent)
        );
        assert!(intent.matches_kind(&kind));
        assert!(!intent.matches_kind(&NativeTrayActionKind::OpenGallery));
    }
    assert_eq!(ExtensionIntentActionId::parse("capture-screen"), None);
}

#[test]
fn shipped_legacy_screenshot_intents_are_kind_checked() {
    let cases = [
        ("capture-screen", ExtensionIntentActionId::ScreenshotScreen),
        (
            "capture-screenshot-screen",
            ExtensionIntentActionId::ScreenshotScreen,
        ),
        (
            "capture-screenshot-active-window",
            ExtensionIntentActionId::ScreenshotActiveWindow,
        ),
        (
            "capture-screenshot-all-screens",
            ExtensionIntentActionId::ScreenshotAllScreens,
        ),
        (
            "capture-screenshot-region",
            ExtensionIntentActionId::ScreenshotRegion,
        ),
    ];
    assert_legacy_descriptors(&cases, &NativeTrayActionKind::CaptureScreenshot);
    assert_eq!(
        ExtensionIntentActionId::from_descriptor(
            "capture-screenshot-screen",
            &NativeTrayActionKind::StartRecording,
        ),
        None
    );
}

#[test]
fn shipped_legacy_recording_intents_are_kind_checked() {
    let cases = [
        (
            "start-recording",
            ExtensionIntentActionId::RecordingStartScreen,
        ),
        (
            "start-recording-screen",
            ExtensionIntentActionId::RecordingStartScreen,
        ),
        (
            "start-recording-active-window",
            ExtensionIntentActionId::RecordingStartActiveWindow,
        ),
        (
            "start-recording-region",
            ExtensionIntentActionId::RecordingStartRegion,
        ),
    ];
    assert_legacy_descriptors(&cases, &NativeTrayActionKind::StartRecording);
}

fn assert_legacy_descriptors(
    cases: &[(&str, ExtensionIntentActionId)],
    kind: &NativeTrayActionKind,
) {
    for (id, expected) in cases {
        assert_eq!(
            ExtensionIntentActionId::from_descriptor(id, kind),
            Some(*expected)
        );
    }
}
