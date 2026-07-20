//! Runtime pairing of the persisted profile with the single mode authority.

#[cfg(any(windows, test))]
use std::sync::{Arc, Mutex, MutexGuard};
use std::{error::Error, fmt};

#[cfg(any(windows, test))]
use super::LocalProfileDraft;
#[cfg(any(windows, test))]
use super::LocalProfileError;
#[cfg(test)]
use super::OperatingMode;
#[cfg(test)]
use super::manual::ManualDestinationSnapshot;
use super::mode::ModeAuthority;
use super::{LocalProfile, ModeEpoch, ModeTransitionError};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SettingsApplyError {
    #[cfg(any(windows, test))]
    InvalidProfile(LocalProfileError),
    Mode(ModeTransitionError),
    #[cfg(any(windows, test))]
    RuntimeUnavailable,
}

impl fmt::Display for SettingsApplyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let category = match self {
            #[cfg(any(windows, test))]
            Self::InvalidProfile(_) => "invalid-profile",
            Self::Mode(_) => "mode-transition",
            #[cfg(any(windows, test))]
            Self::RuntimeUnavailable => "runtime-unavailable",
        };
        formatter.write_str(category)
    }
}

impl Error for SettingsApplyError {}

#[derive(Clone, Debug)]
pub(crate) struct ActiveSettings {
    #[cfg(any(windows, test))]
    profile: Arc<Mutex<LocalProfile>>,
    mode: ModeAuthority,
}

#[cfg(any(windows, test))]
#[derive(Clone, Debug)]
pub(crate) struct PreparedProfileTransition {
    next_profile: LocalProfile,
    switching_epoch: ModeEpoch,
}

impl ActiveSettings {
    #[must_use]
    pub(crate) fn extension_managed_default() -> Self {
        let profile = super::profile::default_extension_profile();
        Self::from_profile(&profile)
    }

    #[must_use]
    pub(crate) fn from_profile(profile: &LocalProfile) -> Self {
        let mode = ModeAuthority::new(profile.operating_mode());
        Self {
            #[cfg(any(windows, test))]
            profile: Arc::new(Mutex::new(profile.clone())),
            mode,
        }
    }

    #[cfg(test)]
    #[must_use]
    pub(crate) fn for_test_mode(mode: OperatingMode) -> Self {
        let profile = super::profile::default_extension_profile();
        Self {
            profile: Arc::new(Mutex::new(profile)),
            mode: ModeAuthority::new(mode),
        }
    }

    pub(crate) fn epoch(&self) -> Result<ModeEpoch, SettingsApplyError> {
        self.mode.snapshot().map_err(SettingsApplyError::Mode)
    }

    #[cfg(any(windows, test))]
    pub(crate) fn profile_snapshot(&self) -> Result<LocalProfile, SettingsApplyError> {
        Ok(self.profile_lock()?.clone())
    }

    #[must_use]
    pub(crate) fn allows_extension_ingress(&self) -> bool {
        matches!(
            self.epoch(),
            Ok(epoch) if epoch.allows_extension_ingress()
        )
    }

    #[cfg(test)]
    pub(crate) fn manual_destination_snapshot(
        &self,
    ) -> Result<(ModeEpoch, ManualDestinationSnapshot), SettingsApplyError> {
        let epoch = self.epoch()?;
        if epoch.stable_mode() != Some(OperatingMode::Manual) {
            return Err(SettingsApplyError::RuntimeUnavailable);
        }
        let destination = self
            .profile_snapshot()?
            .manual_destination()
            .ok_or(SettingsApplyError::RuntimeUnavailable)?;
        Ok((epoch, destination))
    }

    #[cfg(any(windows, test))]
    pub(crate) fn prepare_profile_transition(
        &self,
        draft: &LocalProfileDraft,
    ) -> Result<PreparedProfileTransition, SettingsApplyError> {
        let current_profile = self.profile_snapshot()?;
        let next_profile = draft
            .materialize(&current_profile)
            .map_err(SettingsApplyError::InvalidProfile)?;
        let stable_epoch = self.epoch()?;
        let switching_epoch = self
            .mode
            .begin_transition(stable_epoch, draft.target_mode())
            .map_err(SettingsApplyError::Mode)?;
        Ok(PreparedProfileTransition {
            next_profile,
            switching_epoch,
        })
    }

    #[cfg(any(windows, test))]
    pub(crate) fn commit_profile_transition(
        &self,
        transition: &PreparedProfileTransition,
    ) -> Result<ModeEpoch, SettingsApplyError> {
        *self.profile_lock()? = transition.next_profile.clone();
        self.mode
            .publish_stable(transition.switching_epoch)
            .map_err(SettingsApplyError::Mode)
    }

    #[cfg(any(windows, test))]
    pub(crate) fn rollback_profile_transition(
        &self,
        transition: &PreparedProfileTransition,
    ) -> Result<ModeEpoch, SettingsApplyError> {
        self.mode
            .rollback_transition(transition.switching_epoch)
            .map_err(SettingsApplyError::Mode)
    }

    #[cfg(test)]
    pub(crate) fn degrade_after_persistence(
        &self,
        transition: &PreparedProfileTransition,
    ) -> Result<ModeEpoch, SettingsApplyError> {
        self.mode
            .degrade_after_persistence(transition.switching_epoch)
            .map_err(SettingsApplyError::Mode)
    }

    #[cfg(any(windows, test))]
    pub(crate) fn degrade_stable(
        &self,
        stable: ModeEpoch,
    ) -> Result<ModeEpoch, SettingsApplyError> {
        self.mode
            .degrade_stable(stable)
            .map_err(SettingsApplyError::Mode)
    }

    #[must_use]
    #[cfg(any(windows, test))]
    pub(crate) const fn switching_epoch(transition: &PreparedProfileTransition) -> ModeEpoch {
        transition.switching_epoch
    }

    #[must_use]
    #[cfg(any(windows, test))]
    pub(crate) fn candidate_profile(transition: &PreparedProfileTransition) -> &LocalProfile {
        &transition.next_profile
    }

    #[cfg(any(windows, test))]
    fn profile_lock(&self) -> Result<MutexGuard<'_, LocalProfile>, SettingsApplyError> {
        self.profile
            .lock()
            .map_err(|_| SettingsApplyError::RuntimeUnavailable)
    }
}
