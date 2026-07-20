//! Recording monotonic timebase source.

use std::{
    sync::OnceLock,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug)]
pub(crate) struct RecordingMonotonicClock {
    now_ns: fn() -> u128,
}

impl Default for RecordingMonotonicClock {
    fn default() -> Self {
        Self {
            now_ns: process_monotonic_ns,
        }
    }
}

impl RecordingMonotonicClock {
    #[cfg(test)]
    pub(crate) const fn fixed(now_ns: fn() -> u128) -> Self {
        Self { now_ns }
    }

    pub(crate) fn now_ns(self) -> u64 {
        let now_ns = (self.now_ns)().max(1);
        u64::try_from(now_ns).unwrap_or(u64::MAX)
    }
}

fn process_monotonic_ns() -> u128 {
    static START: OnceLock<Instant> = OnceLock::new();
    START
        .get_or_init(Instant::now)
        .elapsed()
        .max(Duration::from_nanos(1))
        .as_nanos()
}
