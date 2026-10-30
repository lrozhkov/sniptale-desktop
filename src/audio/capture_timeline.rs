//! Preserve device-frame gaps without allocating an unbounded silence packet.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CaptureTimelineError {
    InvalidRate,
    InvalidPacket,
    PositionRegression,
    PacketPending,
    Overflow,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct CaptureEmission {
    pub(super) frames: u32,
    pub(super) timestamp_100ns: i64,
    pub(super) duration_100ns: i64,
    pub(super) silence: bool,
}

#[derive(Debug)]
pub(super) struct CaptureTimeline {
    rate: u32,
    next_device_frame: u64,
    emitted_frames: u64,
    gap_frames: u64,
    packet_frames: Option<u32>,
}

impl CaptureTimeline {
    pub(super) fn new(rate: u32) -> Result<Self, CaptureTimelineError> {
        if rate == 0 {
            return Err(CaptureTimelineError::InvalidRate);
        }
        Ok(Self {
            rate,
            next_device_frame: 0,
            emitted_frames: 0,
            gap_frames: 0,
            packet_frames: None,
        })
    }

    pub(super) fn queue_packet(
        &mut self,
        device_frame: u64,
        frames: u32,
    ) -> Result<u64, CaptureTimelineError> {
        if self.packet_frames.is_some() {
            return Err(CaptureTimelineError::PacketPending);
        }
        if frames == 0 {
            return Err(CaptureTimelineError::InvalidPacket);
        }
        let gap = device_frame
            .checked_sub(self.next_device_frame)
            .ok_or(CaptureTimelineError::PositionRegression)?;
        let end = device_frame
            .checked_add(u64::from(frames))
            .ok_or(CaptureTimelineError::Overflow)?;
        frame_time(end, self.rate)?;
        self.next_device_frame = end;
        self.gap_frames = gap;
        self.packet_frames = Some(frames);
        Ok(gap)
    }

    pub(super) fn next_emission(
        &mut self,
    ) -> Result<Option<CaptureEmission>, CaptureTimelineError> {
        let Some(packet_frames) = self.packet_frames else {
            return Ok(None);
        };
        let silence = self.gap_frames > 0;
        // Ten milliseconds per poll keeps allocation and the existing media-pump batch bounded.
        let frames = if silence {
            u32::try_from(self.gap_frames.min(u64::from((self.rate / 100).max(1))))
                .map_err(|_| CaptureTimelineError::Overflow)?
        } else {
            packet_frames
        };
        let end = self
            .emitted_frames
            .checked_add(u64::from(frames))
            .ok_or(CaptureTimelineError::Overflow)?;
        let timestamp_100ns = frame_time(self.emitted_frames, self.rate)?;
        let duration_100ns = frame_time(end, self.rate)?
            .checked_sub(timestamp_100ns)
            .filter(|duration| *duration > 0)
            .ok_or(CaptureTimelineError::Overflow)?;
        self.emitted_frames = end;
        if silence {
            self.gap_frames -= u64::from(frames);
        } else {
            self.packet_frames = None;
        }
        Ok(Some(CaptureEmission {
            frames,
            timestamp_100ns,
            duration_100ns,
            silence,
        }))
    }
}

fn frame_time(frames: u64, rate: u32) -> Result<i64, CaptureTimelineError> {
    i64::try_from(u128::from(frames) * 10_000_000 / u128::from(rate))
        .map_err(|_| CaptureTimelineError::Overflow)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_timeline_preserves_a_device_gap_instead_of_shortening_audio()
    -> Result<(), CaptureTimelineError> {
        let mut clock = CaptureTimeline::new(48_000)?;
        clock.queue_packet(0, 480)?;
        assert!(clock.next_emission()?.is_some_and(|packet| !packet.silence));
        // A loaded-guest MP4 lost approximately 4.555 seconds from its audio track.
        assert_eq!(clock.queue_packet(219_120, 480)?, 218_640);
        let mut padding = 0;
        let mut end = 100_000;
        while let Some(packet) = clock.next_emission()? {
            assert_eq!(packet.timestamp_100ns, end);
            end += packet.duration_100ns;
            if packet.silence {
                assert!(packet.frames <= 480);
                padding += packet.frames;
            }
        }
        assert_eq!(padding, 218_640);
        assert_eq!(end, 45_750_000);
        clock.queue_packet(219_600, 480)?;
        assert_eq!(
            clock.next_emission()?.map(|packet| packet.timestamp_100ns),
            Some(end)
        );
        Ok(())
    }

    #[test]
    fn capture_timeline_emits_a_long_gap_lazily_in_bounded_steps()
    -> Result<(), CaptureTimelineError> {
        let mut clock = CaptureTimeline::new(48_000)?;
        clock.queue_packet(48_000 * 86_400, 480)?;
        for expected_start in [0, 100_000] {
            let packet = clock
                .next_emission()?
                .ok_or(CaptureTimelineError::InvalidPacket)?;
            assert!(packet.silence);
            assert_eq!(packet.frames, 480);
            assert_eq!(packet.timestamp_100ns, expected_start);
            assert_eq!(packet.duration_100ns, 100_000);
        }
        assert_eq!(
            clock.queue_packet(0, 480),
            Err(CaptureTimelineError::PacketPending)
        );
        Ok(())
    }

    #[test]
    fn capture_timeline_retains_initial_silence_and_fractional_frame_rounding()
    -> Result<(), CaptureTimelineError> {
        let mut clock = CaptureTimeline::new(44_100)?;
        clock.queue_packet(44_100, 1)?;
        let mut end = 0;
        while let Some(packet) = clock.next_emission()? {
            assert_eq!(packet.timestamp_100ns, end);
            end += packet.duration_100ns;
        }
        for position in 44_101..44_200 {
            clock.queue_packet(position, 1)?;
            let packet = clock
                .next_emission()?
                .ok_or(CaptureTimelineError::InvalidPacket)?;
            assert_eq!(packet.timestamp_100ns, end);
            assert!(!packet.silence);
            end += packet.duration_100ns;
        }
        assert_eq!(end, frame_time(44_200, 44_100)?);
        Ok(())
    }

    #[test]
    fn capture_timeline_rejects_invalid_pending_backwards_and_overflow_packets()
    -> Result<(), CaptureTimelineError> {
        assert!(matches!(
            CaptureTimeline::new(0),
            Err(CaptureTimelineError::InvalidRate)
        ));
        let mut clock = CaptureTimeline::new(48_000)?;
        assert_eq!(
            clock.queue_packet(0, 0),
            Err(CaptureTimelineError::InvalidPacket)
        );
        assert_eq!(
            clock.queue_packet(u64::MAX, 1),
            Err(CaptureTimelineError::Overflow)
        );
        assert_eq!(
            clock.queue_packet(i64::MAX.unsigned_abs(), 1),
            Err(CaptureTimelineError::Overflow)
        );
        clock.queue_packet(0, 480)?;
        assert_eq!(
            clock.queue_packet(480, 1),
            Err(CaptureTimelineError::PacketPending)
        );
        clock.next_emission()?;
        assert_eq!(
            clock.queue_packet(479, 1),
            Err(CaptureTimelineError::PositionRegression)
        );
        assert_eq!(clock.queue_packet(480, 480)?, 0);
        assert!(clock.next_emission()?.is_some_and(|packet| !packet.silence));
        assert_eq!(clock.next_emission()?, None);
        Ok(())
    }
}
