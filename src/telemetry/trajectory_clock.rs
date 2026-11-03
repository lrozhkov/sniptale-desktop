//! Source-clock mapping and constant-memory pause removal, without timestamp repair.

use crate::protocol::trajectory::{
    CursorTrajectoryClock, CursorTrajectoryClockSource, MAX_SAFE_INTEGER,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TrajectoryClockError {
    BeforeStart,
    Paused,
    LateObservation,
    InvalidLifecycle,
    Unrepresentable,
}

/// Extend only the source timestamp. The reference selects its wrap epoch.
pub(crate) fn extend_windows_message_time(
    time: u32,
    reference_ms: u64,
) -> Result<u64, TrajectoryClockError> {
    let period = 1_u64 << 32;
    let base = reference_ms & !(period - 1);
    let middle = base
        .checked_add(u64::from(time))
        .ok_or(TrajectoryClockError::Unrepresentable)?;
    let mut best = middle;
    for candidate in [middle.checked_sub(period), middle.checked_add(period)]
        .into_iter()
        .flatten()
    {
        if candidate.abs_diff(reference_ms) < best.abs_diff(reference_ms) {
            best = candidate;
        }
    }
    if best.abs_diff(reference_ms) >= period / 2 {
        return Err(TrajectoryClockError::Unrepresentable);
    }
    Ok(best)
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct WindowsTrajectoryClock {
    source_anchor_ms: u64,
    recording_anchor_ns: u64,
}

impl WindowsTrajectoryClock {
    pub(crate) fn calibrate(
        source_anchor_ms: u64,
        before_ns: u64,
        after_ns: u64,
    ) -> Result<Self, TrajectoryClockError> {
        let bracket = after_ns
            .checked_sub(before_ns)
            .ok_or(TrajectoryClockError::Unrepresentable)?;
        Ok(Self {
            source_anchor_ms,
            recording_anchor_ns: before_ns + bracket / 2,
        })
    }

    pub(crate) fn event_monotonic_ns(
        self,
        source_time_ms: u64,
    ) -> Result<u64, TrajectoryClockError> {
        let delta_ms = i128::from(source_time_ms) - i128::from(self.source_anchor_ms);
        u64::try_from(i128::from(self.recording_anchor_ns) + delta_ms * 1_000_000)
            .map_err(|_| TrajectoryClockError::BeforeStart)
    }

    pub(crate) const fn metadata() -> CursorTrajectoryClock {
        CursorTrajectoryClock {
            source: CursorTrajectoryClockSource::WindowsMessageTime,
            // Win32 message timestamps are integer milliseconds. Their actual resolution and
            // the boot-clock/recording-clock mapping error have no established numeric bound.
            timestamp_quantum_us: Some(1000),
            resolution_bound_us: None,
            mapping_uncertainty_us: None,
        }
    }

    pub(crate) fn source_cutoff_ms(self, recording_ns: u64) -> Result<u64, TrajectoryClockError> {
        let delta = i128::from(recording_ns) - i128::from(self.recording_anchor_ns);
        u64::try_from(i128::from(self.source_anchor_ms) + delta.div_euclid(1_000_000))
            .map_err(|_| TrajectoryClockError::BeforeStart)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TrajectoryActiveClock {
    started: u64,
    completed_pause: u64,
    pause_started: Option<u64>,
    ordered_floor: u64,
}

impl TrajectoryActiveClock {
    pub(crate) const fn new(started_ns: u64) -> Self {
        Self {
            started: started_ns,
            completed_pause: 0,
            pause_started: None,
            ordered_floor: started_ns,
        }
    }

    pub(crate) fn observe(&mut self, source_ns: u64) -> Result<u64, TrajectoryClockError> {
        if source_ns < self.started {
            return Err(TrajectoryClockError::BeforeStart);
        }
        if self.pause_started.is_some() {
            return Err(TrajectoryClockError::Paused);
        }
        if source_ns < self.ordered_floor {
            return Err(TrajectoryClockError::LateObservation);
        }
        let active = self.project(source_ns)?;
        self.ordered_floor = source_ns;
        Ok(active)
    }

    pub(crate) fn pause(&mut self, source_ns: u64) -> Result<u64, TrajectoryClockError> {
        if self.pause_started.is_some() || source_ns < self.ordered_floor {
            return Err(TrajectoryClockError::InvalidLifecycle);
        }
        let active = self.project(source_ns)?;
        self.pause_started = Some(source_ns);
        self.ordered_floor = source_ns;
        Ok(active)
    }

    pub(crate) fn resume(&mut self, source_ns: u64) -> Result<u64, TrajectoryClockError> {
        let pause = self
            .pause_started
            .ok_or(TrajectoryClockError::InvalidLifecycle)?;
        let duration = source_ns
            .checked_sub(pause)
            .ok_or(TrajectoryClockError::InvalidLifecycle)?;
        let total = self
            .completed_pause
            .checked_add(duration)
            .ok_or(TrajectoryClockError::Unrepresentable)?;
        let active = source_ns
            .checked_sub(self.started)
            .and_then(|value| value.checked_sub(total))
            .ok_or(TrajectoryClockError::InvalidLifecycle)?
            / 1000;
        if active > MAX_SAFE_INTEGER {
            return Err(TrajectoryClockError::Unrepresentable);
        }
        self.completed_pause = total;
        self.pause_started = None;
        self.ordered_floor = source_ns;
        Ok(active)
    }

    pub(crate) fn stop(&self, source_ns: u64) -> Result<u64, TrajectoryClockError> {
        if source_ns < self.ordered_floor {
            return Err(TrajectoryClockError::InvalidLifecycle);
        }
        self.project(self.pause_started.unwrap_or(source_ns))
    }

    fn project(&self, source_ns: u64) -> Result<u64, TrajectoryClockError> {
        let active = source_ns
            .checked_sub(self.started)
            .and_then(|elapsed| elapsed.checked_sub(self.completed_pause))
            .ok_or(TrajectoryClockError::BeforeStart)?
            / 1000;
        if active > MAX_SAFE_INTEGER {
            return Err(TrajectoryClockError::Unrepresentable);
        }
        Ok(active)
    }
}
