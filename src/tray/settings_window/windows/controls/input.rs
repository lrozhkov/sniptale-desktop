//! Typed combo selection and shortcut-recorder input lifecycle.

use super::*;
use crate::tray::settings_window::{
    hotkey_capture::{CaptureKey, CaptureOutcome},
    model::SettingsWindowEvent,
};

impl SettingsNativeWindow {
    pub(in crate::tray::settings_window::windows) fn begin_hotkey_capture(
        &mut self,
        action_index: usize,
    ) {
        let Some((action, _, field)) = self.pages.shortcuts.hotkeys.get(action_index) else {
            return;
        };
        self.capture.begin(*action);
        let _ = native::set_text(*field, "Press shortcut…");
        native::focus(self.handle);
        invalidate(*field);
    }

    pub(in crate::tray::settings_window::windows) fn capture_key(
        &mut self,
        key: CaptureKey,
        key_up: bool,
    ) -> Option<SettingsWindowEvent> {
        if key_up {
            if let CaptureKey::Modifier(modifier) = key {
                self.capture.modifier_up(modifier);
            }
            return None;
        }
        let active = self.capture.active_action();
        match self.capture.key_down(key) {
            CaptureOutcome::Ignored => None,
            CaptureOutcome::Preview(text) => {
                self.set_active_hotkey_text(active, &text);
                None
            }
            CaptureOutcome::Invalid => {
                self.set_active_hotkey_text(active, "Add a modifier");
                None
            }
            CaptureOutcome::Cancel(action) => {
                presentation::refresh_hotkeys(self);
                Some(SettingsWindowEvent::CancelHotkeyCapture(action))
            }
            CaptureOutcome::Clear(action) => Some(SettingsWindowEvent::SetHotkey {
                action,
                chord: None,
            }),
            CaptureOutcome::Commit { action, chord } => Some(SettingsWindowEvent::SetHotkey {
                action,
                chord: Some(chord),
            }),
        }
    }

    pub(in crate::tray::settings_window::windows) fn capture_active(&self) -> bool {
        self.capture.active_action().is_some()
    }

    pub(in crate::tray::settings_window::windows) fn cancel_hotkey_capture(&mut self) {
        if self.capture.cancel().is_some() {
            presentation::refresh_hotkeys(self);
        }
    }

    pub(in crate::tray::settings_window::windows) fn combo_event(
        &self,
        id: usize,
        index: usize,
    ) -> Option<SettingsWindowEvent> {
        match id {
            MICROPHONE_DEVICE_ID => self
                .presentation
                .microphone_selection(index)
                .map(SettingsWindowEvent::SelectMicrophone),
            AUDIO_SOURCE_ID => self
                .presentation
                .audio_source_selection(index)
                .map(SettingsWindowEvent::SetAudioSource),
            VIDEO_BITRATE_ID => self
                .presentation
                .video_bitrate_options()
                .get(index)
                .map(|option| SettingsWindowEvent::SetVideoBitrate(option.value())),
            VIDEO_DURATION_ID => self
                .presentation
                .video_duration_options()
                .get(index)
                .map(|option| SettingsWindowEvent::SetVideoDuration(option.value())),
            _ => super::super::event_map::combo(id, index),
        }
    }

    fn set_active_hotkey_text(
        &self,
        action: Option<crate::settings::ManualHotkeyAction>,
        text: &str,
    ) {
        if let Some((_, _, field)) = self
            .pages
            .shortcuts
            .hotkeys
            .iter()
            .find(|(candidate, _, _)| Some(*candidate) == action)
        {
            let _ = native::set_text(*field, text);
            invalidate(*field);
        }
    }
}
