//! Staged mode-switch presentation from first-party tray commands.

use crate::tray::{
    TrayCommandSource, TrayNoticeKind, TrayRuntimeError,
    settings_window::model::SettingsWindowModel,
};

pub(in crate::app::resident::tray) fn open_extension_managed(
    active_settings: &crate::settings::ActiveSettings,
    model: &mut SettingsWindowModel,
    source: &mut impl TrayCommandSource,
) {
    let result = model
        .show(
            active_settings,
            crate::settings::ui_preferences_health_from_path(
                crate::app::platform::ui_preferences_path(),
            ),
        )
        .map_err(|_| TrayRuntimeError::RenderFailed)
        .map(|_| {
            model.refresh_audio_capabilities(crate::audio::runtime_audio_capability_summary());
            model.stage_extension_managed();
            model.presentation()
        })
        .and_then(|presentation| source.show_settings_window(presentation));
    if result.is_err() {
        let _ = source.show_notice(TrayNoticeKind::ActionRejected);
    }
}

/// Starts Manual setup with the one value that cannot be inferred safely: its publication folder.
/// Cancelling the picker leaves the current authority untouched and does not open a dead-end window.
pub(in crate::app::resident::tray) fn open_manual_setup(
    active_settings: &crate::settings::ActiveSettings,
    model: &mut SettingsWindowModel,
    source: &mut impl TrayCommandSource,
) {
    let result = model
        .show(
            active_settings,
            crate::settings::ui_preferences_health_from_path(
                crate::app::platform::ui_preferences_path(),
            ),
        )
        .map_err(|_| TrayRuntimeError::RenderFailed)
        .and_then(|_| {
            model.refresh_audio_capabilities(crate::audio::runtime_audio_capability_summary());
            source.pick_settings_destination()
        })
        .and_then(|destination| {
            let Some(destination) = destination else {
                model.discard_and_hide();
                return Ok(None);
            };
            model.stage_manual_destination(
                "manual-destination".to_owned(),
                destination.to_string_lossy().into_owned(),
            );
            source
                .show_settings_window(model.presentation())
                .map(|()| Some(()))
        });
    if result.is_err() {
        model.discard_and_hide();
        let _ = source.show_notice(TrayNoticeKind::ActionRejected);
    }
}
