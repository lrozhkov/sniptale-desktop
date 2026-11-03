//! Independent producer times may arrive out of order inside a joined lifecycle segment.

use super::trajectory_clock::TrajectoryClockError;
use crate::protocol::trajectory::MAX_SAFE_INTEGER;

#[derive(Clone, Copy, Debug)]
pub(crate) struct EventActiveClock {
    started: u64,
    completed_pause: u64,
    pause_started: Option<u64>,
    phase_floor: u64,
    latest_observed: u64,
}

impl EventActiveClock {
    pub(crate) const fn new(started: u64) -> Self {
        Self {
            started,
            completed_pause: 0,
            pause_started: None,
            phase_floor: started,
            latest_observed: started,
        }
    }

    pub(crate) fn observe(&mut self, source: u64) -> Result<u64, TrajectoryClockError> {
        if source < self.started {
            return Err(TrajectoryClockError::BeforeStart);
        }
        if self.pause_started.is_some() {
            return Err(TrajectoryClockError::Paused);
        }
        if source < self.phase_floor {
            return Err(TrajectoryClockError::LateObservation);
        }
        let active = self.project(source)?;
        self.latest_observed = self.latest_observed.max(source);
        Ok(active)
    }

    pub(crate) fn pause(&mut self, source: u64) -> Result<u64, TrajectoryClockError> {
        if self.pause_started.is_some() || source < self.latest_observed {
            return Err(TrajectoryClockError::InvalidLifecycle);
        }
        let active = self.project(source)?;
        self.pause_started = Some(source);
        self.phase_floor = source;
        self.latest_observed = source;
        Ok(active)
    }

    pub(crate) fn resume(&mut self, source: u64) -> Result<u64, TrajectoryClockError> {
        let pause = self
            .pause_started
            .ok_or(TrajectoryClockError::InvalidLifecycle)?;
        let duration = source
            .checked_sub(pause)
            .ok_or(TrajectoryClockError::InvalidLifecycle)?;
        let completed = self
            .completed_pause
            .checked_add(duration)
            .ok_or(TrajectoryClockError::Unrepresentable)?;
        let mut candidate = *self;
        candidate.completed_pause = completed;
        let active = candidate.project(source)?;
        candidate.pause_started = None;
        candidate.phase_floor = source;
        candidate.latest_observed = source;
        *self = candidate;
        Ok(active)
    }

    pub(crate) fn stop(&self, source: u64) -> Result<u64, TrajectoryClockError> {
        if source < self.latest_observed {
            return Err(TrajectoryClockError::InvalidLifecycle);
        }
        self.project(self.pause_started.unwrap_or(source))
    }

    fn project(&self, source: u64) -> Result<u64, TrajectoryClockError> {
        let active = source
            .checked_sub(self.started)
            .and_then(|delta| delta.checked_sub(self.completed_pause))
            .ok_or(TrajectoryClockError::BeforeStart)?
            / 1000;
        if active > MAX_SAFE_INTEGER {
            return Err(TrajectoryClockError::Unrepresentable);
        }
        Ok(active)
    }
}
