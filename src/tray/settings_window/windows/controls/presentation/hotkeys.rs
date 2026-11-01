//! Staged hotkey labels projected into native controls.

use super::{SettingsNativeWindow, native};

pub(in crate::tray::settings_window::windows::controls) fn refresh_hotkeys(
    window: &SettingsNativeWindow,
) {
    let hotkeys = window.presentation.manual().map(|manual| &manual.hotkeys);
    for (action, _, control) in &window.pages.shortcuts.hotkeys {
        let label = hotkeys
            .and_then(|hotkeys| hotkeys.get(action.as_str()))
            .and_then(|value| crate::settings::parse_manual_hotkey_chord(value))
            .map_or_else(
                || "Not set".to_owned(),
                crate::settings::ManualHotkeyChord::canonical_label,
            );
        let _ = native::set_text(*control, &label);
        native::invalidate(*control);
    }
}
