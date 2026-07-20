use crate::{
    settings::action_catalog_from_native_actions,
    tray::{
        ExtensionIntentActionId, ShellActionId, TrayActionError, TrayActionKind,
        TrayActionRegistry, TrayActionRequest, TrayMenuItem, registry_from_action_catalog,
    },
};

use super::{native_registry, registry};

#[test]
fn shell_action_ids_are_stable_and_native_only() {
    let ids = [
        ShellActionId::ScreenshotScreen,
        ShellActionId::ScreenshotActiveWindow,
        ShellActionId::ScreenshotAllScreens,
        ShellActionId::ScreenshotRegion,
        ShellActionId::RecordingStartScreen,
        ShellActionId::RecordingStartActiveWindow,
        ShellActionId::RecordingStartRegion,
        ShellActionId::RecordingPause,
        ShellActionId::RecordingResume,
        ShellActionId::RecordingStop,
        ShellActionId::RecordingCancel,
        ShellActionId::OpenSettings,
        ShellActionId::ModeExtensionManaged,
        ShellActionId::ModeManual,
        ShellActionId::OpenSaveFolder,
        ShellActionId::OpenExtensionSettings,
        ShellActionId::RetryPendingSaves,
        ShellActionId::DiscardPendingSaves,
        ShellActionId::Quit,
    ];
    assert_eq!(ids.len(), 19);
    assert!(ids.iter().all(|id| !id.as_str().is_empty()));
    assert_eq!(ShellActionId::OpenSettings.as_str(), "open-settings");
    assert_eq!(
        ShellActionId::ScreenshotScreen.extension_intent(),
        Some(ExtensionIntentActionId::ScreenshotScreen)
    );
    assert_eq!(
        ShellActionId::RecordingStartRegion.extension_intent(),
        Some(ExtensionIntentActionId::RecordingStartRegion)
    );
    assert_eq!(ShellActionId::RecordingPause.extension_intent(), None);
    assert_eq!(ShellActionId::Quit.extension_intent(), None);
}

#[test]
fn action_catalog_maps_to_local_action_registry() -> Result<(), String> {
    let catalog = action_catalog_from_native_actions(&native_registry())
        .map_err(|error| format!("{error:?}"))?;
    let registry = registry_from_action_catalog(&catalog);

    assert_eq!(registry.actions().len(), 2);
    assert_eq!(registry.actions()[0].id(), "screenshot-screen");
    assert_eq!(
        registry.actions()[0].action_kind(),
        Some(TrayActionKind::CaptureScreenshot)
    );
    assert_eq!(registry.actions()[0].shortcut_label(), Some("Ctrl+Shift+S"));
    assert_eq!(registry.actions()[1].id(), "open-gallery");
    assert!(registry.actions()[1].offline_capable());
    Ok(())
}

#[test]
fn action_request_does_not_authorize_work_by_itself() -> Result<(), TrayActionError> {
    let request = registry().request_action("screenshot-screen", true, "invocation-1", 10)?;

    assert_eq!(
        request,
        TrayActionRequest {
            action_id: "screenshot-screen".to_owned(),
            invocation_id: "invocation-1".to_owned(),
            requested_at_epoch_ms: 10,
            kind: TrayActionKind::CaptureScreenshot,
        }
    );
    Ok(())
}

#[test]
fn extension_intent_request_requires_the_checked_registry_projection() -> Result<(), TrayActionError>
{
    let registry = TrayActionRegistry::new(vec![
        TrayMenuItem::configured(
            ExtensionIntentActionId::ScreenshotScreen.as_str(),
            "Current screen",
            TrayActionKind::CaptureScreenshot,
        )
        .with_extension_intent(Some(ExtensionIntentActionId::ScreenshotScreen)),
    ]);

    let request = registry.request_extension_intent(
        ShellActionId::ScreenshotScreen,
        true,
        "invocation-1",
        10,
    )?;

    assert_eq!(request.action_id, "screenshot-screen");
    assert!(matches!(
        registry.request_extension_intent(ShellActionId::RecordingPause, true, "invocation-2", 11),
        Err(TrayActionError::UnknownAction)
    ));
    Ok(())
}

#[test]
fn extension_intent_projection_preserves_an_accepted_legacy_wire_id() -> Result<(), TrayActionError>
{
    let registry = TrayActionRegistry::new(vec![
        TrayMenuItem::configured(
            "capture-screen",
            "Current screen",
            TrayActionKind::CaptureScreenshot,
        )
        .with_extension_intent(Some(ExtensionIntentActionId::ScreenshotScreen)),
    ]);

    let request = registry.request_extension_intent(
        ShellActionId::ScreenshotScreen,
        true,
        "invocation-legacy",
        10,
    )?;

    assert_eq!(request.action_id, "capture-screen");
    Ok(())
}

#[test]
fn disconnected_action_requires_offline_capability() {
    assert!(matches!(
        registry().request_action("screenshot-screen", false, "invocation-1", 10),
        Err(TrayActionError::Disconnected)
    ));
    assert!(
        registry()
            .request_action("open-gallery", false, "invocation-2", 10)
            .is_ok()
    );
}
