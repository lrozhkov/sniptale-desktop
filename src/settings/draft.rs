//! Typed profile candidates validated before runtime ingress is fenced.

use super::{
    LocalProfile, LocalProfileError, ManualProfileDraft, OperatingMode,
    profile::NATIVE_LOCAL_PROFILE_SCHEMA_VERSION, profile::validate_profile,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LocalProfileDraft {
    operating_mode: OperatingMode,
    manual_profile: Option<ManualProfileDraft>,
}

impl LocalProfileDraft {
    #[must_use]
    pub(crate) const fn extension_managed() -> Self {
        Self {
            operating_mode: OperatingMode::ExtensionManaged,
            manual_profile: None,
        }
    }

    #[must_use]
    pub(crate) fn manual(manual_profile: ManualProfileDraft) -> Self {
        Self {
            operating_mode: OperatingMode::Manual,
            manual_profile: Some(manual_profile),
        }
    }

    #[must_use]
    pub(crate) const fn target_mode(&self) -> OperatingMode {
        self.operating_mode
    }

    /// Changes only the Manual output destination while preserving every other staged setting.
    pub(crate) fn set_manual_destination(
        &mut self,
        destination_id: String,
        destination_path: String,
    ) {
        if let Some(manual_profile) = self.manual_profile.as_mut() {
            manual_profile.set_destination(destination_id, destination_path);
        }
    }

    pub(crate) fn manual_mut(&mut self) -> Option<&mut ManualProfileDraft> {
        self.manual_profile.as_mut()
    }

    #[must_use]
    pub(crate) fn manual_view(&self) -> Option<super::ManualProfileDraftView> {
        self.manual_profile.as_ref().map(ManualProfileDraft::view)
    }

    pub(super) fn materialize(
        &self,
        current: &LocalProfile,
    ) -> Result<LocalProfile, LocalProfileError> {
        let revision = current
            .revision()
            .checked_add(1)
            .ok_or(LocalProfileError::InvalidProfile)?;
        let candidate = LocalProfile {
            schema_version: NATIVE_LOCAL_PROFILE_SCHEMA_VERSION,
            revision,
            operating_mode: self.operating_mode,
            manual_profile: self
                .manual_profile
                .clone()
                .map(ManualProfileDraft::into_profile),
        };
        validate_profile(&candidate)?;
        Ok(candidate)
    }
}

impl From<&LocalProfile> for LocalProfileDraft {
    fn from(profile: &LocalProfile) -> Self {
        Self {
            operating_mode: profile.operating_mode(),
            manual_profile: profile
                .manual_profile
                .as_ref()
                .map(ManualProfileDraft::from_profile),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::LocalProfileDraft;
    use crate::settings::manual::model::ManualAudioSource;
    use crate::settings::{ManualProfileDraft, profile::default_extension_profile};

    #[test]
    fn destination_edit_preserves_other_manual_values() -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let before = directory.path().join("before");
        let after = directory.path().join("after");
        let mut manual = ManualProfileDraft::with_defaults(
            "before".to_owned(),
            before.to_string_lossy().into_owned(),
        );
        manual.set_audio(ManualAudioSource::Microphone, Some("mic-1".to_owned()), 96);
        let mut draft = LocalProfileDraft::manual(manual);

        draft.set_manual_destination("after".to_owned(), after.to_string_lossy().into_owned());

        let profile = draft
            .materialize(&default_extension_profile())
            .map_err(|_| std::io::Error::other("profile"))?;
        let destination = profile
            .manual_destination()
            .ok_or_else(|| std::io::Error::other("destination"))?;
        let recording = profile
            .manual_recording_settings()
            .ok_or_else(|| std::io::Error::other("recording"))?;
        assert_eq!(destination.id, "after");
        assert_eq!(destination.root, after);
        assert_eq!(recording.microphone_device_id.as_deref(), Some("mic-1"));
        Ok(())
    }
}
