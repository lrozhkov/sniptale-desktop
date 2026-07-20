//! Typed settings events derived from native control ids and virtual keys.

use super::*;
use crate::tray::settings_window::{hotkey_capture::CaptureKey, model::SettingsWindowSection};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    VIRTUAL_KEY, VK_BACK, VK_CONTROL, VK_DELETE, VK_ESCAPE, VK_LCONTROL, VK_LMENU, VK_LSHIFT,
    VK_LWIN, VK_MENU, VK_RCONTROL, VK_RMENU, VK_RSHIFT, VK_RWIN, VK_SHIFT, VK_SNAPSHOT,
};

pub(super) fn button(id: usize) -> Option<SettingsWindowEvent> {
    match id {
        BROWSE_DESTINATION_ID => Some(SettingsWindowEvent::BrowseDestination),
        EXTENSION_MODE_ID => Some(SettingsWindowEvent::UseExtensionManaged),
        SCREENSHOT_CURSOR_ID => Some(SettingsWindowEvent::ToggleScreenshotCursor),
        VIDEO_CURSOR_ID => Some(SettingsWindowEvent::ToggleVideoCursor),
        REFRESH_MICROPHONES_ID => Some(SettingsWindowEvent::RefreshMicrophones),
        APPLY_ID => Some(SettingsWindowEvent::Apply),
        CANCEL_ID => Some(SettingsWindowEvent::RequestClose),
        CONFIRM_DISCARD_ID => Some(SettingsWindowEvent::ConfirmDiscard),
        KEEP_EDITING_ID => Some(SettingsWindowEvent::KeepEditing),
        NAV_FIRST_ID..=33 => SettingsWindowSection::ALL
            .get(id - NAV_FIRST_ID)
            .copied()
            .map(SettingsWindowEvent::Navigate),
        _ => None,
    }
}

pub(super) fn combo(id: usize, index: usize) -> Option<SettingsWindowEvent> {
    match id {
        THEME_ID => theme(index),
        VIDEO_QUALITY_ID | VIDEO_FPS_ID | VIDEO_HARDWARE_ID => video_combo(id, index),
        AUDIO_BITRATE_ID => audio_combo(id, index),
        _ => None,
    }
}

fn theme(index: usize) -> Option<SettingsWindowEvent> {
    [
        crate::settings::AppearanceThemePreference::System,
        crate::settings::AppearanceThemePreference::Light,
        crate::settings::AppearanceThemePreference::Dark,
    ]
    .get(index)
    .copied()
    .map(SettingsWindowEvent::SelectAppearanceTheme)
}

fn video_combo(id: usize, index: usize) -> Option<SettingsWindowEvent> {
    match id {
        VIDEO_QUALITY_ID => [
            crate::settings::ManualVideoQuality::Low,
            crate::settings::ManualVideoQuality::Medium,
            crate::settings::ManualVideoQuality::High,
            crate::settings::ManualVideoQuality::Ultra,
        ]
        .get(index)
        .copied()
        .map(SettingsWindowEvent::SetVideoQuality),
        VIDEO_FPS_ID => [
            crate::settings::ManualFps::Fps24,
            crate::settings::ManualFps::Fps30,
            crate::settings::ManualFps::Fps60,
        ]
        .get(index)
        .copied()
        .map(SettingsWindowEvent::SetVideoFps),
        VIDEO_HARDWARE_ID => [
            crate::settings::ManualHardwarePreference::Prefer,
            crate::settings::ManualHardwarePreference::ForceSoftware,
        ]
        .get(index)
        .copied()
        .map(SettingsWindowEvent::SetVideoHardware),
        _ => None,
    }
}

fn audio_combo(id: usize, index: usize) -> Option<SettingsWindowEvent> {
    match id {
        AUDIO_BITRATE_ID => [96, 128, 160, 192]
            .get(index)
            .copied()
            .map(SettingsWindowEvent::SetAudioBitrate),
        _ => None,
    }
}

pub(super) fn capture_key(key: VIRTUAL_KEY) -> CaptureKey {
    match key {
        VK_CONTROL | VK_LCONTROL | VK_RCONTROL => {
            CaptureKey::Modifier(crate::settings::ManualHotkeyChord::CTRL)
        }
        VK_SHIFT | VK_LSHIFT | VK_RSHIFT => {
            CaptureKey::Modifier(crate::settings::ManualHotkeyChord::SHIFT)
        }
        VK_MENU | VK_LMENU | VK_RMENU => {
            CaptureKey::Modifier(crate::settings::ManualHotkeyChord::ALT)
        }
        VK_LWIN | VK_RWIN => CaptureKey::Modifier(crate::settings::ManualHotkeyChord::META),
        VK_ESCAPE => CaptureKey::Escape,
        VK_DELETE => CaptureKey::Delete,
        VK_BACK => CaptureKey::Backspace,
        VK_SNAPSHOT => CaptureKey::PrintScreen,
        _ if (0x30..=0x39).contains(&key.0) || (0x41..=0x5a).contains(&key.0) => {
            char::from_u32(u32::from(key.0)).map_or(CaptureKey::Unsupported, CaptureKey::Character)
        }
        _ if (0x70..=0x87).contains(&key.0) => CaptureKey::Function((key.0 - 0x70 + 1) as u8),
        _ => CaptureKey::Unsupported,
    }
}
