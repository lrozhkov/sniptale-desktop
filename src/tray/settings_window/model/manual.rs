//! Staged Manual settings mutations and local validation feedback.

use super::*;
use crate::settings::{
    ManualAudioSource, ManualFps, ManualHardwarePreference, ManualHotkeyAction, ManualHotkeyChord,
    ManualHotkeyUpdateError, ManualVideoQuality,
};

impl SettingsWindowModel {
    pub(crate) fn set_screenshot_include_cursor(&mut self, include_cursor: bool) {
        self.with_manual(|manual| manual.set_screenshot_include_cursor(include_cursor));
    }

    pub(crate) fn set_video_quality(&mut self, value: ManualVideoQuality) {
        self.with_manual(|manual| manual.set_video_quality(value));
    }

    pub(crate) fn set_video_fps(&mut self, value: ManualFps) {
        self.with_manual(|manual| manual.set_video_fps(value));
    }

    pub(crate) fn set_video_bitrate(&mut self, value: Option<u16>) {
        self.with_manual(|manual| manual.set_video_bitrate_override(value));
    }

    pub(crate) fn set_video_hardware(&mut self, value: ManualHardwarePreference) {
        self.with_manual(|manual| manual.set_video_hardware_preference(value));
    }

    pub(crate) fn toggle_video_cursor(&mut self) {
        if let Some(view) = self.manual_view() {
            self.with_manual(|manual| manual.set_video_include_cursor(!view.video_include_cursor));
        }
    }

    pub(crate) fn set_video_duration(&mut self, value: u32) {
        self.with_manual(|manual| manual.set_video_max_duration_seconds(value));
    }

    pub(crate) fn set_audio_source(&mut self, value: ManualAudioSource) {
        if self.audio_source_available(value) {
            self.with_manual(|manual| manual.set_audio_source_mode(value));
        } else {
            self.feedback = Some(SettingsWindowFeedback::AudioSourceUnavailable);
        }
    }

    pub(crate) fn set_microphone_device_id(&mut self, value: Option<String>) {
        self.with_manual(|manual| manual.set_microphone_device_id(value));
    }

    pub(crate) fn set_audio_bitrate(&mut self, value: u16) {
        self.with_manual(|manual| manual.set_audio_bitrate_kbps(value));
    }

    pub(crate) fn set_hotkey(
        &mut self,
        action: ManualHotkeyAction,
        chord: Option<ManualHotkeyChord>,
    ) {
        let result = self
            .draft
            .as_mut()
            .and_then(LocalProfileDraft::manual_mut)
            .map(|manual| manual.set_hotkey(action, chord));
        self.feedback = match result {
            Some(Err(ManualHotkeyUpdateError::Duplicate(existing))) => {
                self.hotkey_conflict = Some((action, existing));
                Some(SettingsWindowFeedback::HotkeyConflict { action, existing })
            }
            Some(Ok(())) => {
                self.hotkey_conflict = None;
                None
            }
            None => None,
        };
    }

    pub(crate) fn cancel_hotkey_edit(&mut self, action: ManualHotkeyAction) {
        if self
            .hotkey_conflict
            .is_some_and(|(conflicting, _)| conflicting == action)
        {
            self.hotkey_conflict = None;
            self.feedback = None;
        }
    }

    pub(crate) fn refresh_audio_capabilities(
        &mut self,
        summary: crate::audio::AudioCapabilitySummary,
    ) {
        let selected = self
            .manual_view()
            .and_then(|manual| manual.microphone_device_id);
        self.audio_support = AudioSourceSupport {
            microphone: summary.supports_microphone,
            system: summary.supports_system_audio,
            mixed: summary.supports_mixed_audio,
        };
        self.microphones = vec![SettingsMicrophoneOption {
            id: None,
            label: "System default".to_owned(),
            available: summary.supports_microphone,
        }];
        self.microphones
            .extend(summary.microphone_devices.into_iter().map(|device| {
                let label = microphone_label(&device);
                SettingsMicrophoneOption {
                    id: Some(device.id),
                    label,
                    available: device.available,
                }
            }));
        if let Some(selected) = selected
            && !self
                .microphones
                .iter()
                .any(|option| option.id.as_deref() == Some(selected.as_str()))
        {
            self.microphones.push(SettingsMicrophoneOption {
                id: Some(selected),
                label: "Previously selected microphone (unavailable)".to_owned(),
                available: false,
            });
        }
        if !self.manual_selection_valid() {
            self.feedback = Some(
                if self
                    .manual_view()
                    .is_some_and(|manual| !self.audio_source_available(manual.audio_source_mode))
                {
                    SettingsWindowFeedback::AudioSourceUnavailable
                } else {
                    SettingsWindowFeedback::MicrophoneUnavailable
                },
            );
        } else if matches!(
            self.feedback,
            Some(
                SettingsWindowFeedback::MicrophoneUnavailable
                    | SettingsWindowFeedback::AudioSourceUnavailable
            )
        ) {
            self.feedback = None;
        }
    }

    pub(crate) fn select_microphone(&mut self, selection: SettingsMicrophoneSelection) {
        let id = match selection {
            SettingsMicrophoneSelection::SystemDefault => None,
            SettingsMicrophoneSelection::Device(id) => Some(id),
        };
        if self
            .microphones
            .iter()
            .any(|option| option.id == id && option.available)
        {
            self.set_microphone_device_id(id);
            self.feedback = None;
        } else {
            self.feedback = Some(SettingsWindowFeedback::MicrophoneUnavailable);
        }
    }

    fn with_manual(&mut self, mutate: impl FnOnce(&mut ManualProfileDraft)) {
        if let Some(manual) = self.draft.as_mut().and_then(LocalProfileDraft::manual_mut) {
            mutate(manual);
            self.feedback = None;
        }
    }
}

fn microphone_label(device: &crate::audio::MicrophoneDeviceDescriptor) -> String {
    match (device.available, device.is_default) {
        (false, _) => format!("{} (unavailable)", device.label),
        (true, true) => format!("{} (default)", device.label),
        (true, false) => device.label.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_selections_update_only_the_staged_profile() -> Result<(), std::io::Error> {
        let mut model = SettingsWindowModel::default();
        model.stage_manual_destination("destination".to_owned(), "C:\\Capture".to_owned());
        model.refresh_audio_capabilities(available_audio());
        model.set_video_quality(ManualVideoQuality::Ultra);
        model.set_video_fps(ManualFps::Fps60);
        model.set_video_bitrate(Some(48));
        model.set_video_hardware(ManualHardwarePreference::ForceSoftware);
        model.toggle_video_cursor();
        model.set_video_duration(900);
        model.set_audio_source(ManualAudioSource::Microphone);
        model.set_audio_bitrate(192);
        model.set_hotkey(
            ManualHotkeyAction::ScreenshotScreen,
            crate::settings::parse_manual_hotkey_chord("Ctrl+Shift+1"),
        );
        let view = model
            .manual_view()
            .ok_or_else(|| std::io::Error::other("staged view"))?;
        assert_eq!(view.video_quality, ManualVideoQuality::Ultra);
        assert_eq!(view.video_fps, ManualFps::Fps60);
        assert_eq!(view.video_bitrate_override_mbps, Some(48));
        assert_eq!(
            view.video_hardware_preference,
            ManualHardwarePreference::ForceSoftware
        );
        assert!(!view.video_include_cursor);
        assert_eq!(view.video_max_duration_seconds, 900);
        assert_eq!(view.audio_source_mode, ManualAudioSource::Microphone);
        assert_eq!(view.audio_bitrate_kbps, 192);
        assert_eq!(
            view.hotkeys.get("screenshot-screen").map(String::as_str),
            Some("Ctrl+Shift+1")
        );
        Ok(())
    }

    #[test]
    fn duplicate_shortcut_is_rejected_without_reassigning_the_existing_action() {
        let mut model = SettingsWindowModel::default();
        model.stage_manual_destination("destination".to_owned(), "C:\\Capture".to_owned());
        model.refresh_audio_capabilities(available_audio());
        let chord = crate::settings::parse_manual_hotkey_chord("Ctrl+Shift+1");
        model.set_hotkey(ManualHotkeyAction::ScreenshotScreen, chord);
        model.set_hotkey(ManualHotkeyAction::ScreenshotRegion, chord);
        assert_eq!(
            model.presentation().feedback(),
            Some(SettingsWindowFeedback::HotkeyConflict {
                action: ManualHotkeyAction::ScreenshotRegion,
                existing: ManualHotkeyAction::ScreenshotScreen,
            })
        );
        let hotkeys = model
            .manual_view()
            .map(|view| view.hotkeys)
            .unwrap_or_default();
        assert_eq!(hotkeys.len(), 1);
        assert!(hotkeys.contains_key("screenshot-screen"));
        assert!(!model.presentation().can_apply());
        model.cancel_hotkey_edit(ManualHotkeyAction::ScreenshotRegion);
        assert!(model.presentation().can_apply());
    }

    fn available_audio() -> crate::audio::AudioCapabilitySummary {
        crate::audio::AudioCapabilitySummary {
            microphone_devices: Vec::new(),
            supports_microphone: true,
            supports_system_audio: true,
            supports_mixed_audio: true,
            unavailable_reasons: Vec::new(),
        }
    }
}
