//! Monotonic timebase to media timeline conversion.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TimebaseError {
    BeforeStart,
    InvalidPause,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PausedInterval {
    pub(crate) start_monotonic_ns: u64,
    pub(crate) end_monotonic_ns: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MediaTimebase {
    started_at_monotonic_ns: u64,
}

impl MediaTimebase {
    #[must_use]
    pub(crate) const fn new(started_at_monotonic_ns: u64) -> Self {
        Self {
            started_at_monotonic_ns,
        }
    }

    pub(crate) fn elapsed_media_ms(
        self,
        monotonic_ns: u64,
        pauses: &[PausedInterval],
    ) -> Result<u64, TimebaseError> {
        let elapsed_ns = monotonic_ns
            .checked_sub(self.started_at_monotonic_ns)
            .ok_or(TimebaseError::BeforeStart)?;
        let paused_ns = paused_duration_before(monotonic_ns, pauses)?;
        elapsed_ns
            .checked_sub(paused_ns)
            .ok_or(TimebaseError::InvalidPause)
            .map(|active_ns| active_ns / 1_000_000)
    }
}

fn paused_duration_before(
    monotonic_ns: u64,
    pauses: &[PausedInterval],
) -> Result<u64, TimebaseError> {
    let mut total = 0_u64;
    for pause in pauses {
        if pause.end_monotonic_ns < pause.start_monotonic_ns {
            return Err(TimebaseError::InvalidPause);
        }
        let pause_end = pause.end_monotonic_ns.min(monotonic_ns);
        if pause_end > pause.start_monotonic_ns {
            total = total
                .checked_add(pause_end - pause.start_monotonic_ns)
                .ok_or(TimebaseError::InvalidPause)?;
        }
    }
    Ok(total)
}
