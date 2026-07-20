//! Immutable settings-window presentation contract.

use crate::settings::{AppearanceThemePreference, ManualHotkeyAction, ManualProfileDraftView};

#[path = "presentation/options.rs"]
mod options;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SettingsWindowView {
    ExtensionManagedReadOnly,
    ExtensionManagedDraft,
    ManualDraft,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum SettingsWindowSection {
    #[default]
    Capture,
    Video,
    Audio,
    Shortcuts,
}

impl SettingsWindowSection {
    pub(crate) const ALL: [Self; 4] = [Self::Capture, Self::Video, Self::Audio, Self::Shortcuts];

    #[must_use]
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Capture => "Capture",
            Self::Video => "Video",
            Self::Audio => "Audio",
            Self::Shortcuts => "Shortcuts",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SettingsWindowFeedback {
    AppearanceSaveFailed,
    SaveFailed,
    RecordingBusy,
    HotkeyUnavailable,
    MicrophoneUnavailable,
    AudioSourceUnavailable,
    HotkeyConflict {
        action: ManualHotkeyAction,
        existing: ManualHotkeyAction,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum SettingsMicrophoneSelection {
    SystemDefault,
    Device(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SettingsMicrophoneOption {
    pub(super) id: Option<String>,
    pub(super) label: String,
    pub(super) available: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SettingsWindowApplyState {
    Pristine,
    Invalid,
    Ready,
}

impl SettingsWindowApplyState {
    pub(super) const fn from_flags(dirty: bool, valid: bool) -> Self {
        match (dirty, valid) {
            (false, _) => Self::Pristine,
            (true, false) => Self::Invalid,
            (true, true) => Self::Ready,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SettingsWindowCloseState {
    Editing,
    ConfirmDiscard,
}

impl SettingsMicrophoneOption {
    #[must_use]
    pub(crate) fn label(&self) -> &str {
        &self.label
    }

    #[must_use]
    pub(crate) const fn available(&self) -> bool {
        self.available
    }

    #[must_use]
    pub(crate) fn selection(&self) -> SettingsMicrophoneSelection {
        self.id.clone().map_or(
            SettingsMicrophoneSelection::SystemDefault,
            SettingsMicrophoneSelection::Device,
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SettingsAudioSourceOption {
    pub(super) source: crate::settings::ManualAudioSource,
    pub(super) available: bool,
}

impl SettingsAudioSourceOption {
    #[must_use]
    pub(crate) const fn source(self) -> crate::settings::ManualAudioSource {
        self.source
    }

    #[must_use]
    pub(crate) const fn available(self) -> bool {
        self.available
    }

    #[must_use]
    pub(crate) const fn label(self) -> &'static str {
        match (self.source, self.available) {
            (crate::settings::ManualAudioSource::Microphone, true) => "Microphone",
            (crate::settings::ManualAudioSource::System, true) => "System audio",
            (crate::settings::ManualAudioSource::Mixed, true) => "Mixed",
            (crate::settings::ManualAudioSource::Microphone, false) => "Microphone (unavailable)",
            (crate::settings::ManualAudioSource::System, false) => "System audio (unavailable)",
            (crate::settings::ManualAudioSource::Mixed, false) => "Mixed (unavailable)",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SettingsWindowPresentation {
    pub(super) view: SettingsWindowView,
    pub(super) manual: Option<ManualProfileDraftView>,
    pub(super) microphones: Vec<SettingsMicrophoneOption>,
    pub(super) audio_sources: Vec<SettingsAudioSourceOption>,
    pub(super) switching_mode: bool,
    pub(super) section: SettingsWindowSection,
    pub(super) theme: AppearanceThemePreference,
    pub(super) apply_state: SettingsWindowApplyState,
    pub(super) feedback: Option<SettingsWindowFeedback>,
    pub(super) close_state: SettingsWindowCloseState,
}

impl SettingsWindowPresentation {
    #[must_use]
    pub(crate) const fn view(&self) -> SettingsWindowView {
        self.view
    }

    #[must_use]
    pub(crate) const fn shows_navigation(&self) -> bool {
        matches!(self.view, SettingsWindowView::ManualDraft)
    }

    #[must_use]
    pub(crate) const fn shows_footer(&self) -> bool {
        !matches!(self.view, SettingsWindowView::ExtensionManagedReadOnly)
    }

    #[must_use]
    pub(crate) const fn shows_manual_setup(&self) -> bool {
        matches!(self.view, SettingsWindowView::ExtensionManagedReadOnly)
    }

    #[must_use]
    pub(crate) const fn section(&self) -> SettingsWindowSection {
        self.section
    }

    #[must_use]
    pub(crate) const fn theme(&self) -> AppearanceThemePreference {
        self.theme
    }

    #[must_use]
    #[cfg(test)]
    pub(crate) const fn dirty(&self) -> bool {
        !matches!(self.apply_state, SettingsWindowApplyState::Pristine)
    }

    #[must_use]
    pub(crate) const fn can_apply(&self) -> bool {
        matches!(self.apply_state, SettingsWindowApplyState::Ready)
    }

    #[must_use]
    pub(crate) const fn feedback(&self) -> Option<SettingsWindowFeedback> {
        self.feedback
    }

    #[must_use]
    pub(crate) const fn confirm_discard(&self) -> bool {
        matches!(self.close_state, SettingsWindowCloseState::ConfirmDiscard)
    }

    #[must_use]
    pub(crate) fn manual(&self) -> Option<&ManualProfileDraftView> {
        self.manual.as_ref()
    }

    #[must_use]
    pub(crate) fn microphones(&self) -> &[SettingsMicrophoneOption] {
        &self.microphones
    }

    #[must_use]
    pub(crate) fn audio_sources(&self) -> &[SettingsAudioSourceOption] {
        &self.audio_sources
    }

    #[must_use]
    pub(crate) fn microphone_selection(&self, index: usize) -> Option<SettingsMicrophoneSelection> {
        self.microphones
            .get(index)
            .map(SettingsMicrophoneOption::selection)
    }

    #[must_use]
    pub(crate) fn audio_source_selection(
        &self,
        index: usize,
    ) -> Option<crate::settings::ManualAudioSource> {
        self.audio_sources.get(index).map(|option| option.source())
    }

    #[must_use]
    pub(crate) fn selected_microphone_index(&self) -> usize {
        let selected = self
            .manual
            .as_ref()
            .and_then(|manual| manual.microphone_device_id.as_deref());
        self.microphones
            .iter()
            .position(|option| option.id.as_deref() == selected)
            .unwrap_or(0)
    }

    #[must_use]
    #[cfg(test)]
    pub(crate) const fn switching_mode(&self) -> bool {
        self.switching_mode
    }

    #[must_use]
    pub(crate) const fn apply_label(&self) -> &'static str {
        match (self.view, self.switching_mode) {
            (SettingsWindowView::ManualDraft, true) => "Save and switch to Manual",
            (SettingsWindowView::ExtensionManagedDraft, true) => "Switch to extension",
            (SettingsWindowView::ManualDraft, false) => "Save changes",
            (SettingsWindowView::ExtensionManagedReadOnly, _)
            | (SettingsWindowView::ExtensionManagedDraft, false) => "Set up Manual mode",
        }
    }
}
