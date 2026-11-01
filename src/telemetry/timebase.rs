//! Millisecond projection of the common constant-memory active recording clock.

use super::trajectory_clock::{TrajectoryActiveClock, TrajectoryClockError};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TimebaseError {
    BeforeStart,
    InvalidPause,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MediaTimebase {
    clock: TrajectoryActiveClock,
}

impl MediaTimebase {
    pub(crate) const fn new(started_at_monotonic_ns: u64) -> Self {
        Self {
            clock: TrajectoryActiveClock::new(started_at_monotonic_ns),
        }
    }

    pub(crate) fn elapsed_media_ms(&mut self, monotonic_ns: u64) -> Result<u64, TimebaseError> {
        self.clock
            .observe(monotonic_ns)
            .map(|microseconds| microseconds / 1000)
            .map_err(map_error)
    }

    pub(crate) fn pause(&mut self, monotonic_ns: u64) -> Result<(), TimebaseError> {
        self.clock
            .pause(monotonic_ns)
            .map(|_| ())
            .map_err(map_error)
    }

    pub(crate) fn resume(&mut self, monotonic_ns: u64) -> Result<(), TimebaseError> {
        self.clock
            .resume(monotonic_ns)
            .map(|_| ())
            .map_err(map_error)
    }
}

fn map_error(error: TrajectoryClockError) -> TimebaseError {
    match error {
        TrajectoryClockError::BeforeStart => TimebaseError::BeforeStart,
        _ => TimebaseError::InvalidPause,
    }
}
