//! Immutable extension-session admission snapshot tied to the mode authority.

use crate::settings::{ActiveSettings, ModeEpoch};

#[derive(Clone, Debug)]
pub(crate) struct ExtensionSessionEpoch {
    settings: ActiveSettings,
    admitted: ModeEpoch,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ExtensionSessionEpochError {
    ExtensionIngressUnavailable,
}

impl ExtensionSessionEpoch {
    pub(crate) fn admit(settings: ActiveSettings) -> Result<Self, ExtensionSessionEpochError> {
        let admitted = settings
            .epoch()
            .map_err(|_| ExtensionSessionEpochError::ExtensionIngressUnavailable)?;
        if !admitted.allows_extension_ingress() {
            return Err(ExtensionSessionEpochError::ExtensionIngressUnavailable);
        }
        Ok(Self { settings, admitted })
    }

    #[must_use]
    pub(crate) fn remains_current(&self) -> bool {
        self.settings
            .epoch()
            .is_ok_and(|epoch| epoch == self.admitted)
    }
}

#[cfg(test)]
mod tests {
    use crate::settings::{
        ActiveSettings, LocalProfileDraft, ManualProfileDraft, OperatingMode,
        ensure_local_profile_from_path,
    };

    use super::{ExtensionSessionEpoch, ExtensionSessionEpochError};

    #[test]
    fn extension_session_snapshot_is_invalidated_by_a_mode_transition()
    -> Result<(), Box<dyn std::error::Error>> {
        let profile =
            ensure_local_profile_from_path(None).map_err(|_| std::io::Error::other("profile"))?;
        let settings = ActiveSettings::from_profile(&profile);
        let gate = ExtensionSessionEpoch::admit(settings.clone())
            .map_err(|_| std::io::Error::other("session gate"))?;
        assert!(gate.remains_current());

        let directory = tempfile::tempdir()?;
        let draft = LocalProfileDraft::manual(ManualProfileDraft::with_defaults(
            "destination".to_owned(),
            directory.path().to_string_lossy().into_owned(),
        ));
        let transition = settings.prepare_profile_transition(&draft)?;
        assert!(!gate.remains_current());
        settings.rollback_profile_transition(&transition)?;
        assert!(!gate.remains_current());
        Ok(())
    }

    #[test]
    fn manual_mode_cannot_admit_an_extension_session() {
        assert!(matches!(
            ExtensionSessionEpoch::admit(ActiveSettings::for_test_mode(OperatingMode::Manual)),
            Err(ExtensionSessionEpochError::ExtensionIngressUnavailable)
        ));
    }
}
