//! Transactional mode-change coordinator around settings authority and runtime barriers.

use crate::settings::{
    ActiveSettings, LocalProfile, LocalProfileDraft, ModeEpoch, SettingsApplyError,
    persist_profile_candidate_from_path,
};

pub(crate) trait ModeTransitionBarrier {
    fn quiesce(&mut self, switching: ModeEpoch) -> Result<(), ModeCoordinatorError>;
    fn stage(&mut self, switching: ModeEpoch) -> Result<(), ModeCoordinatorError>;
    fn publish(&mut self, stable: ModeEpoch) -> Result<(), ModeCoordinatorError>;
    fn rollback(&mut self, stable: ModeEpoch) -> Result<(), ModeCoordinatorError>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ModeCoordinatorError {
    Settings(SettingsApplyError),
    Barrier,
}

pub(crate) fn apply_profile_draft_with_barrier(
    path: Option<std::path::PathBuf>,
    active: &ActiveSettings,
    draft: &LocalProfileDraft,
    barrier: &mut impl ModeTransitionBarrier,
) -> Result<LocalProfile, ModeCoordinatorError> {
    let transition = active
        .prepare_profile_transition(draft)
        .map_err(ModeCoordinatorError::Settings)?;
    let switching = ActiveSettings::switching_epoch(&transition);
    if barrier.quiesce(switching).is_err() {
        let stable = active
            .rollback_profile_transition(&transition)
            .map_err(ModeCoordinatorError::Settings)?;
        let _ = barrier.rollback(stable);
        return Err(ModeCoordinatorError::Barrier);
    }
    if barrier.stage(switching).is_err() {
        let stable = active
            .rollback_profile_transition(&transition)
            .map_err(ModeCoordinatorError::Settings)?;
        let _ = barrier.rollback(stable);
        return Err(ModeCoordinatorError::Barrier);
    }
    let candidate = ActiveSettings::candidate_profile(&transition).clone();
    if let Err(error) = persist_profile_candidate_from_path(path, &candidate) {
        let stable = active
            .rollback_profile_transition(&transition)
            .map_err(ModeCoordinatorError::Settings)?;
        let _ = barrier.rollback(stable);
        return Err(ModeCoordinatorError::Settings(
            SettingsApplyError::InvalidProfile(error),
        ));
    }
    let stable = active
        .commit_profile_transition(&transition)
        .map_err(ModeCoordinatorError::Settings)?;
    if barrier.publish(stable).is_err() {
        let _ = active.degrade_stable(stable);
        return Err(ModeCoordinatorError::Barrier);
    }
    Ok(candidate)
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::{ModeCoordinatorError, ModeTransitionBarrier, apply_profile_draft_with_barrier};
    use crate::settings::{
        ActiveSettings, LocalProfileDraft, ManualProfileDraft, ModeEpoch, OperatingMode,
        ensure_local_profile_from_path,
    };

    #[derive(Default)]
    struct Barrier {
        staged: bool,
        published: bool,
        rollback: bool,
        rejection: Rejection,
    }

    #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
    enum Rejection {
        #[default]
        None,
        Quiesce,
        Stage,
    }

    impl ModeTransitionBarrier for Barrier {
        fn quiesce(&mut self, _switching: ModeEpoch) -> Result<(), ModeCoordinatorError> {
            if self.rejection == Rejection::Quiesce {
                Err(ModeCoordinatorError::Barrier)
            } else {
                Ok(())
            }
        }

        fn stage(&mut self, _switching: ModeEpoch) -> Result<(), ModeCoordinatorError> {
            self.staged = true;
            if self.rejection == Rejection::Stage {
                Err(ModeCoordinatorError::Barrier)
            } else {
                Ok(())
            }
        }

        fn publish(&mut self, _stable: ModeEpoch) -> Result<(), ModeCoordinatorError> {
            self.published = true;
            Ok(())
        }

        fn rollback(&mut self, _stable: ModeEpoch) -> Result<(), ModeCoordinatorError> {
            self.rollback = true;
            Ok(())
        }
    }

    #[test]
    fn coordinator_fences_then_persists_before_publishing_runtime_mode()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempdir()?;
        let path = directory.path().join("local-profile.json");
        let profile = ensure_local_profile_from_path(Some(path.clone()))
            .map_err(|error| std::io::Error::other(format!("{error:?}")))?;
        let active = ActiveSettings::from_profile(&profile);
        let draft = LocalProfileDraft::manual(ManualProfileDraft::with_defaults(
            "destination".to_owned(),
            directory.path().to_string_lossy().into_owned(),
        ));
        let mut barrier = Barrier::default();

        let committed = apply_profile_draft_with_barrier(Some(path), &active, &draft, &mut barrier)
            .map_err(|error| std::io::Error::other(format!("{error:?}")))?;

        assert_eq!(committed.operating_mode(), OperatingMode::Manual);
        assert!(barrier.staged);
        assert!(barrier.published);
        assert!(!barrier.rollback);
        assert_eq!(active.epoch()?.stable_mode(), Some(OperatingMode::Manual));
        Ok(())
    }

    #[test]
    fn coordinator_rolls_back_the_mode_fence_when_quiescence_is_rejected()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempdir()?;
        let path = directory.path().join("local-profile.json");
        let profile = ensure_local_profile_from_path(Some(path.clone()))
            .map_err(|error| std::io::Error::other(format!("{error:?}")))?;
        let active = ActiveSettings::from_profile(&profile);
        let draft = LocalProfileDraft::manual(ManualProfileDraft::with_defaults(
            "destination".to_owned(),
            directory.path().to_string_lossy().into_owned(),
        ));
        let mut barrier = Barrier {
            rejection: Rejection::Quiesce,
            ..Barrier::default()
        };

        assert_eq!(
            apply_profile_draft_with_barrier(Some(path), &active, &draft, &mut barrier),
            Err(ModeCoordinatorError::Barrier)
        );
        assert!(barrier.rollback);
        assert!(!barrier.staged);
        assert_eq!(
            active.epoch()?.stable_mode(),
            Some(OperatingMode::ExtensionManaged)
        );
        Ok(())
    }

    #[test]
    fn coordinator_restores_runtime_projection_when_profile_persistence_fails()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempdir()?;
        let profile = ensure_local_profile_from_path(None)
            .map_err(|error| std::io::Error::other(format!("{error:?}")))?;
        let active = ActiveSettings::from_profile(&profile);
        let draft = LocalProfileDraft::manual(ManualProfileDraft::with_defaults(
            "destination".to_owned(),
            directory.path().to_string_lossy().into_owned(),
        ));
        let mut barrier = Barrier::default();
        let blocked_parent = directory.path().join("not-a-directory");
        std::fs::write(&blocked_parent, b"block")?;

        let result = apply_profile_draft_with_barrier(
            Some(blocked_parent.join("profile.json")),
            &active,
            &draft,
            &mut barrier,
        );

        assert!(matches!(result, Err(ModeCoordinatorError::Settings(_))));
        assert!(barrier.rollback);
        assert!(barrier.staged);
        assert!(!barrier.published);
        assert_eq!(
            active.epoch()?.stable_mode(),
            Some(OperatingMode::ExtensionManaged)
        );
        Ok(())
    }

    #[test]
    fn rejected_runtime_stage_restores_old_authority_without_persisting_candidate()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempdir()?;
        let path = directory.path().join("local-profile.json");
        let profile = ensure_local_profile_from_path(Some(path.clone()))
            .map_err(|error| std::io::Error::other(format!("{error:?}")))?;
        let original = profile.clone();
        let active = ActiveSettings::from_profile(&profile);
        let draft = LocalProfileDraft::manual(ManualProfileDraft::with_defaults(
            "destination".to_owned(),
            directory.path().to_string_lossy().into_owned(),
        ));
        let mut barrier = Barrier {
            rejection: Rejection::Stage,
            ..Barrier::default()
        };

        assert_eq!(
            apply_profile_draft_with_barrier(Some(path.clone()), &active, &draft, &mut barrier),
            Err(ModeCoordinatorError::Barrier)
        );
        assert!(barrier.staged);
        assert!(barrier.rollback);
        assert!(!barrier.published);
        assert_eq!(
            ensure_local_profile_from_path(Some(path))
                .map_err(|error| std::io::Error::other(format!("{error:?}")))?,
            original
        );
        assert_eq!(
            active.epoch()?.stable_mode(),
            Some(OperatingMode::ExtensionManaged)
        );
        Ok(())
    }
}
