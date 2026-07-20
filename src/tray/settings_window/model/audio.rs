//! Capability-aware audio validity and presentation choices.

use super::*;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct AudioSourceSupport {
    pub(super) microphone: bool,
    pub(super) system: bool,
    pub(super) mixed: bool,
}

impl SettingsWindowModel {
    pub(super) fn manual_selection_valid(&self) -> bool {
        let Some(manual) = self.manual_view() else {
            return self.draft.is_some();
        };
        if !self.audio_source_available(manual.audio_source_mode) {
            return false;
        }
        if matches!(
            manual.audio_source_mode,
            crate::settings::ManualAudioSource::System
        ) {
            return true;
        }
        self.microphones
            .get(self.selected_microphone_index())
            .is_some_and(SettingsMicrophoneOption::available)
    }

    pub(super) fn audio_source_available(
        &self,
        source: crate::settings::ManualAudioSource,
    ) -> bool {
        match source {
            crate::settings::ManualAudioSource::Microphone => self.audio_support.microphone,
            crate::settings::ManualAudioSource::System => self.audio_support.system,
            crate::settings::ManualAudioSource::Mixed => self.audio_support.mixed,
        }
    }

    pub(super) fn audio_source_options(&self) -> Vec<SettingsAudioSourceOption> {
        let selected = self.manual_view().map(|manual| manual.audio_source_mode);
        [
            crate::settings::ManualAudioSource::Microphone,
            crate::settings::ManualAudioSource::System,
            crate::settings::ManualAudioSource::Mixed,
        ]
        .into_iter()
        .filter(|source| self.audio_source_available(*source) || selected == Some(*source))
        .map(|source| SettingsAudioSourceOption {
            source,
            available: self.audio_source_available(source),
        })
        .collect()
    }

    fn selected_microphone_index(&self) -> usize {
        let selected = self
            .manual_view()
            .and_then(|manual| manual.microphone_device_id);
        self.microphones
            .iter()
            .position(|option| option.id == selected)
            .unwrap_or(0)
    }
}
