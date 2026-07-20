//! Monotonic WGC frame pacing for the recording runtime.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum VideoFrameClockError {
    InvalidFrameDuration,
    InvalidCaptureTimestamp,
    TimestampOverflow,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct VideoFrameClock {
    frame_duration: i64,
    capture_epoch: Option<i64>,
    last_capture_timestamp: Option<i64>,
    next_due: i64,
}

impl VideoFrameClock {
    pub(super) fn new(frame_duration_100ns: u64) -> Result<Self, VideoFrameClockError> {
        let frame_duration = i64::try_from(frame_duration_100ns)
            .map_err(|_| VideoFrameClockError::InvalidFrameDuration)?;
        if frame_duration <= 0 {
            return Err(VideoFrameClockError::InvalidFrameDuration);
        }
        Ok(Self {
            frame_duration,
            capture_epoch: None,
            last_capture_timestamp: None,
            next_due: 0,
        })
    }

    pub(super) fn timestamp_for_capture(
        &mut self,
        capture_timestamp_100ns: i64,
    ) -> Result<Option<i64>, VideoFrameClockError> {
        if capture_timestamp_100ns < 0
            || self
                .last_capture_timestamp
                .is_some_and(|last| capture_timestamp_100ns < last)
        {
            return Err(VideoFrameClockError::InvalidCaptureTimestamp);
        }
        let epoch = *self.capture_epoch.get_or_insert(capture_timestamp_100ns);
        self.last_capture_timestamp = Some(capture_timestamp_100ns);
        let elapsed = capture_timestamp_100ns
            .checked_sub(epoch)
            .ok_or(VideoFrameClockError::TimestampOverflow)?;
        self.timestamp_for_elapsed(elapsed)
    }

    pub(super) fn timestamp_for_repeat(
        &mut self,
        elapsed_100ns: i64,
    ) -> Result<Option<i64>, VideoFrameClockError> {
        if self.capture_epoch.is_none() || elapsed_100ns < 0 {
            return Ok(None);
        }
        self.timestamp_for_elapsed(elapsed_100ns)
    }

    fn timestamp_for_elapsed(
        &mut self,
        elapsed_100ns: i64,
    ) -> Result<Option<i64>, VideoFrameClockError> {
        if elapsed_100ns < self.next_due {
            return Ok(None);
        }
        let intervals = elapsed_100ns
            .checked_sub(self.next_due)
            .and_then(|delta| delta.checked_div(self.frame_duration))
            .and_then(|skipped| skipped.checked_add(1))
            .ok_or(VideoFrameClockError::TimestampOverflow)?;
        self.next_due = intervals
            .checked_mul(self.frame_duration)
            .and_then(|advance| self.next_due.checked_add(advance))
            .ok_or(VideoFrameClockError::TimestampOverflow)?;
        Ok(Some(elapsed_100ns))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_clock_uses_capture_elapsed_time_and_drops_early_frames()
    -> Result<(), VideoFrameClockError> {
        let mut clock = VideoFrameClock::new(333_333)?;
        assert_eq!(clock.timestamp_for_capture(50_000_000), Ok(Some(0)));
        assert_eq!(clock.timestamp_for_capture(50_100_000), Ok(None));
        assert_eq!(clock.timestamp_for_capture(50_340_000), Ok(Some(340_000)));
        assert_eq!(
            clock.timestamp_for_capture(75_000_000),
            Ok(Some(25_000_000))
        );
        Ok(())
    }

    #[test]
    fn frame_clock_rejects_invalid_duration_and_backwards_capture_time()
    -> Result<(), VideoFrameClockError> {
        assert_eq!(
            VideoFrameClock::new(0),
            Err(VideoFrameClockError::InvalidFrameDuration)
        );
        let mut clock = VideoFrameClock::new(333_333)?;
        assert_eq!(clock.timestamp_for_capture(10), Ok(Some(0)));
        assert_eq!(
            clock.timestamp_for_capture(9),
            Err(VideoFrameClockError::InvalidCaptureTimestamp)
        );
        Ok(())
    }

    #[test]
    fn frame_clock_repeats_the_last_frame_only_when_the_next_frame_is_due()
    -> Result<(), VideoFrameClockError> {
        let mut clock = VideoFrameClock::new(333_333)?;
        assert_eq!(clock.timestamp_for_repeat(333_333), Ok(None));
        assert_eq!(clock.timestamp_for_capture(50_000_000), Ok(Some(0)));
        assert_eq!(clock.timestamp_for_repeat(100_000), Ok(None));
        assert_eq!(clock.timestamp_for_repeat(333_333), Ok(Some(333_333)));
        assert_eq!(clock.timestamp_for_repeat(340_000), Ok(None));
        assert_eq!(clock.timestamp_for_repeat(1_000_000), Ok(Some(1_000_000)));
        Ok(())
    }
}
