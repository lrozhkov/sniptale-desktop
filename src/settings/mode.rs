//! Single-writer runtime authority for persisted operating-mode changes.

use std::sync::{Arc, Mutex, MutexGuard};

use super::OperatingMode;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ModePhase {
    Stable(OperatingMode),
    #[cfg(any(windows, test))]
    Switching {
        previous: OperatingMode,
        target: OperatingMode,
    },
    #[cfg(any(windows, test))]
    Degraded(OperatingMode),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ModeEpoch {
    phase: ModePhase,
    #[cfg(any(windows, test))]
    generation: u64,
}

impl ModeEpoch {
    #[must_use]
    pub(crate) const fn stable(mode: OperatingMode) -> Self {
        Self {
            phase: ModePhase::Stable(mode),
            #[cfg(any(windows, test))]
            generation: 1,
        }
    }

    #[must_use]
    #[cfg(test)]
    pub(crate) const fn phase(self) -> ModePhase {
        self.phase
    }

    #[must_use]
    #[cfg(test)]
    pub(crate) const fn generation(self) -> u64 {
        self.generation
    }

    #[cfg(any(windows, test))]
    #[must_use]
    pub(crate) const fn stable_mode(self) -> Option<OperatingMode> {
        match self.phase {
            ModePhase::Stable(mode) => Some(mode),
            #[cfg(any(windows, test))]
            ModePhase::Switching { .. } | ModePhase::Degraded(_) => None,
        }
    }

    #[must_use]
    pub(crate) const fn allows_extension_ingress(self) -> bool {
        matches!(
            self.phase,
            ModePhase::Stable(OperatingMode::ExtensionManaged)
        )
    }

    #[must_use]
    #[cfg(test)]
    pub(crate) const fn accepts_new_operations(self) -> bool {
        self.stable_mode().is_some()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ModeTransitionError {
    #[cfg(any(windows, test))]
    StaleEpoch,
    #[cfg(any(windows, test))]
    TransitionInProgress,
    #[cfg(any(windows, test))]
    GenerationExhausted,
    RuntimeUnavailable,
}

/// Cloneable handle to the sole mutable mode state for one resident process.
#[derive(Clone, Debug)]
pub(crate) struct ModeAuthority {
    epoch: Arc<Mutex<ModeEpoch>>,
}

impl ModeAuthority {
    #[must_use]
    pub(crate) fn new(mode: OperatingMode) -> Self {
        Self {
            epoch: Arc::new(Mutex::new(ModeEpoch::stable(mode))),
        }
    }

    pub(crate) fn snapshot(&self) -> Result<ModeEpoch, ModeTransitionError> {
        Ok(*self.lock()?)
    }

    #[cfg(any(windows, test))]
    pub(crate) fn begin_transition(
        &self,
        expected: ModeEpoch,
        target: OperatingMode,
    ) -> Result<ModeEpoch, ModeTransitionError> {
        let mut current = self.lock()?;
        if *current != expected {
            return Err(ModeTransitionError::StaleEpoch);
        }
        let ModePhase::Stable(previous) = expected.phase else {
            return Err(ModeTransitionError::TransitionInProgress);
        };
        let generation = expected
            .generation
            .checked_add(1)
            .ok_or(ModeTransitionError::GenerationExhausted)?;
        let switching = ModeEpoch {
            phase: ModePhase::Switching { previous, target },
            generation,
        };
        *current = switching;
        Ok(switching)
    }

    #[cfg(any(windows, test))]
    pub(crate) fn publish_stable(
        &self,
        switching: ModeEpoch,
    ) -> Result<ModeEpoch, ModeTransitionError> {
        let mut current = self.lock()?;
        if *current != switching {
            return Err(ModeTransitionError::StaleEpoch);
        }
        let ModePhase::Switching { target, .. } = switching.phase else {
            return Err(ModeTransitionError::TransitionInProgress);
        };
        let stable = ModeEpoch {
            phase: ModePhase::Stable(target),
            generation: switching.generation,
        };
        *current = stable;
        Ok(stable)
    }

    #[cfg(any(windows, test))]
    pub(crate) fn rollback_transition(
        &self,
        switching: ModeEpoch,
    ) -> Result<ModeEpoch, ModeTransitionError> {
        let mut current = self.lock()?;
        if *current != switching {
            return Err(ModeTransitionError::StaleEpoch);
        }
        let ModePhase::Switching { previous, .. } = switching.phase else {
            return Err(ModeTransitionError::TransitionInProgress);
        };
        let stable = ModeEpoch {
            phase: ModePhase::Stable(previous),
            generation: switching.generation,
        };
        *current = stable;
        Ok(stable)
    }

    #[cfg(test)]
    pub(crate) fn degrade_after_persistence(
        &self,
        switching: ModeEpoch,
    ) -> Result<ModeEpoch, ModeTransitionError> {
        let mut current = self.lock()?;
        if *current != switching {
            return Err(ModeTransitionError::StaleEpoch);
        }
        let ModePhase::Switching { target, .. } = switching.phase else {
            return Err(ModeTransitionError::TransitionInProgress);
        };
        let degraded = ModeEpoch {
            phase: ModePhase::Degraded(target),
            generation: switching.generation,
        };
        *current = degraded;
        Ok(degraded)
    }

    #[cfg(any(windows, test))]
    pub(crate) fn degrade_stable(
        &self,
        stable: ModeEpoch,
    ) -> Result<ModeEpoch, ModeTransitionError> {
        let mut current = self.lock()?;
        if *current != stable {
            return Err(ModeTransitionError::StaleEpoch);
        }
        let ModePhase::Stable(mode) = stable.phase else {
            return Err(ModeTransitionError::TransitionInProgress);
        };
        let degraded = ModeEpoch {
            phase: ModePhase::Degraded(mode),
            generation: stable.generation,
        };
        *current = degraded;
        Ok(degraded)
    }

    fn lock(&self) -> Result<MutexGuard<'_, ModeEpoch>, ModeTransitionError> {
        self.epoch
            .lock()
            .map_err(|_| ModeTransitionError::RuntimeUnavailable)
    }
}
