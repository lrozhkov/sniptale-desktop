//! Pure modeless-settings presentation state.

use crate::settings::{
    ActiveSettings, AppearanceThemePreference, LocalProfileDraft, ManualProfileDraft,
    OperatingMode, SettingsApplyError, UiPreferencesHealth,
};

#[path = "model/audio.rs"]
mod audio;
#[path = "model/manual.rs"]
mod manual;
#[path = "model/presentation.rs"]
mod presentation;
pub(crate) use presentation::{
    SettingsAudioSourceOption, SettingsMicrophoneOption, SettingsMicrophoneSelection,
    SettingsWindowFeedback, SettingsWindowPresentation, SettingsWindowSection, SettingsWindowView,
};
#[cfg(test)]
#[path = "model/presentation_tests.rs"]
mod presentation_tests;
#[cfg(test)]
#[path = "model/tests.rs"]
mod tests;

use audio::AudioSourceSupport;

#[cfg(any(windows, test))]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum SettingsWindowEvent {
    Navigate(SettingsWindowSection),
    BrowseDestination,
    ToggleScreenshotCursor,
    SetVideoQuality(crate::settings::ManualVideoQuality),
    SetVideoFps(crate::settings::ManualFps),
    SetVideoBitrate(Option<u16>),
    SetVideoHardware(crate::settings::ManualHardwarePreference),
    ToggleVideoCursor,
    SetVideoDuration(u32),
    SetAudioSource(crate::settings::ManualAudioSource),
    SelectMicrophone(SettingsMicrophoneSelection),
    SetAudioBitrate(u16),
    SetHotkey {
        action: crate::settings::ManualHotkeyAction,
        chord: Option<crate::settings::ManualHotkeyChord>,
    },
    CancelHotkeyCapture(crate::settings::ManualHotkeyAction),
    RefreshMicrophones,
    SelectAppearanceTheme(AppearanceThemePreference),
    UseExtensionManaged,
    Apply,
    RequestClose,
    ConfirmDiscard,
    KeepEditing,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SettingsWindowModelError {
    SettingsUnavailable,
    ManualDraftRequired,
    NoChanges,
    InvalidDraft,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CloseDisposition {
    Hide,
    ConfirmDiscard,
}

#[derive(Clone, Debug)]
pub(crate) struct SettingsWindowModel {
    visible: bool,
    draft: Option<LocalProfileDraft>,
    baseline: Option<LocalProfileDraft>,
    active_mode: Option<OperatingMode>,
    microphones: Vec<SettingsMicrophoneOption>,
    audio_support: AudioSourceSupport,
    section: SettingsWindowSection,
    appearance: AppearanceThemePreference,
    feedback: Option<SettingsWindowFeedback>,
    hotkey_conflict: Option<(
        crate::settings::ManualHotkeyAction,
        crate::settings::ManualHotkeyAction,
    )>,
    confirm_discard: bool,
}

impl Default for SettingsWindowModel {
    fn default() -> Self {
        Self {
            visible: false,
            draft: None,
            baseline: None,
            active_mode: None,
            microphones: vec![SettingsMicrophoneOption {
                id: None,
                label: "System default".to_owned(),
                available: false,
            }],
            audio_support: AudioSourceSupport::default(),
            section: SettingsWindowSection::Capture,
            appearance: AppearanceThemePreference::System,
            feedback: None,
            hotkey_conflict: None,
            confirm_discard: false,
        }
    }
}

impl SettingsWindowModel {
    pub(crate) fn show(
        &mut self,
        active: &ActiveSettings,
        appearance: UiPreferencesHealth,
    ) -> Result<SettingsWindowView, SettingsWindowModelError> {
        let profile = active.profile_snapshot().map_err(map_settings_error)?;
        self.visible = true;
        self.active_mode = Some(profile.operating_mode());
        self.baseline = (profile.operating_mode() == OperatingMode::Manual)
            .then(|| LocalProfileDraft::from(&profile));
        self.draft.clone_from(&self.baseline);
        self.section = SettingsWindowSection::Capture;
        self.appearance = appearance.effective_theme();
        self.feedback = None;
        self.hotkey_conflict = None;
        self.confirm_discard = false;
        Ok(self.view())
    }

    #[must_use]
    pub(crate) fn view(&self) -> SettingsWindowView {
        match self.draft.as_ref().map(LocalProfileDraft::target_mode) {
            Some(OperatingMode::Manual) => SettingsWindowView::ManualDraft,
            Some(OperatingMode::ExtensionManaged) => SettingsWindowView::ExtensionManagedDraft,
            None => SettingsWindowView::ExtensionManagedReadOnly,
        }
    }

    #[must_use]
    pub(crate) fn manual_view(&self) -> Option<crate::settings::ManualProfileDraftView> {
        self.draft.as_ref().and_then(LocalProfileDraft::manual_view)
    }

    #[must_use]
    pub(crate) fn dirty(&self) -> bool {
        self.draft != self.baseline
    }

    #[must_use]
    pub(crate) fn presentation(&self) -> SettingsWindowPresentation {
        let dirty = self.dirty();
        let valid = self.manual_selection_valid();
        SettingsWindowPresentation {
            view: self.view(),
            manual: self.manual_view(),
            microphones: self.microphones.clone(),
            audio_sources: self.audio_source_options(),
            switching_mode: self
                .draft
                .as_ref()
                .is_some_and(|draft| Some(draft.target_mode()) != self.active_mode),
            section: self.section,
            theme: self.appearance,
            apply_state: presentation::SettingsWindowApplyState::from_flags(
                dirty,
                valid && self.hotkey_conflict.is_none(),
            ),
            feedback: self.feedback,
            close_state: if self.confirm_discard {
                presentation::SettingsWindowCloseState::ConfirmDiscard
            } else {
                presentation::SettingsWindowCloseState::Editing
            },
        }
    }

    pub(crate) fn navigate(&mut self, section: SettingsWindowSection) {
        self.section = section;
        self.confirm_discard = false;
    }

    pub(crate) fn stage_manual(&mut self, draft: LocalProfileDraft) {
        self.draft = Some(draft);
        self.feedback = None;
        self.hotkey_conflict = None;
    }

    pub(crate) fn stage_extension_managed(&mut self) {
        self.draft = Some(LocalProfileDraft::extension_managed());
        self.feedback = None;
        self.hotkey_conflict = None;
    }

    pub(crate) fn stage_manual_destination(&mut self, destination_id: String, destination: String) {
        if self.view() == SettingsWindowView::ManualDraft {
            if let Some(draft) = self.draft.as_mut() {
                draft.set_manual_destination(destination_id, destination);
            }
        } else {
            self.stage_manual(LocalProfileDraft::manual(
                ManualProfileDraft::with_defaults(destination_id, destination),
            ));
        }
        self.feedback = None;
    }

    pub(crate) fn appearance_saved(&mut self, theme: AppearanceThemePreference) {
        self.appearance = theme;
        self.feedback = None;
    }

    pub(crate) fn appearance_save_failed(&mut self) {
        self.feedback = Some(SettingsWindowFeedback::AppearanceSaveFailed);
    }

    pub(crate) fn set_apply_feedback(&mut self, feedback: SettingsWindowFeedback) {
        self.feedback = Some(feedback);
    }

    pub(crate) fn apply_candidate(&self) -> Result<LocalProfileDraft, SettingsWindowModelError> {
        if !self.dirty() {
            return Err(SettingsWindowModelError::NoChanges);
        }
        if !self.manual_selection_valid() || self.hotkey_conflict.is_some() {
            return Err(SettingsWindowModelError::InvalidDraft);
        }
        self.draft
            .clone()
            .ok_or(SettingsWindowModelError::ManualDraftRequired)
    }

    pub(crate) fn request_close(&mut self) -> CloseDisposition {
        if self.dirty() {
            self.confirm_discard = true;
            CloseDisposition::ConfirmDiscard
        } else {
            self.discard_and_hide();
            CloseDisposition::Hide
        }
    }

    pub(crate) fn keep_editing(&mut self) {
        self.confirm_discard = false;
    }

    pub(crate) fn discard_and_hide(&mut self) {
        self.visible = false;
        self.draft = None;
        self.baseline = None;
        self.active_mode = None;
        self.feedback = None;
        self.hotkey_conflict = None;
        self.confirm_discard = false;
    }
}

const fn map_settings_error(_error: SettingsApplyError) -> SettingsWindowModelError {
    SettingsWindowModelError::SettingsUnavailable
}
