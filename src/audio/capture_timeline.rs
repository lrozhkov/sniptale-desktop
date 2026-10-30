//! Preserve device-frame gaps without allocating an unbounded silence packet.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CaptureTimelineError {
    InvalidRate,
    InvalidPacket,
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
    device_rate: u32,
    next_device_frame: u64,
    emitted_frames: u64,
    gap_frames: u64,
    packet_frames: Option<u32>,
}

impl CaptureTimeline {
    pub(super) fn new(rate: u32, device_rate: u32) -> Result<Self, CaptureTimelineError> {
        if rate == 0 || device_rate == 0 {
            return Err(CaptureTimelineError::InvalidRate);
        }
        Ok(Self {
            rate,
            device_rate,
            next_device_frame: 0,
            emitted_frames: 0,
            gap_frames: 0,
            packet_frames: None,
        })
    }

    pub(super) fn queue_packet(
        &mut self,
        device_frame: Option<u64>,
        frames: u32,
    ) -> Result<u64, CaptureTimelineError> {
        if self.packet_frames.is_some() {
            return Err(CaptureTimelineError::PacketPending);
        }
        if frames == 0 {
            return Err(CaptureTimelineError::InvalidPacket);
        }
        // Driver positions are advisory. A reset or overlapping position must not discard an
        // otherwise valid PCM packet. An uncertain timestamp continues from accepted frames.
        let device_frame = match device_frame {
            Some(position) => u64::try_from(
                u128::from(position) * u128::from(self.rate) / u128::from(self.device_rate),
            )
            .map_err(|_| CaptureTimelineError::Overflow)?,
            None => self.next_device_frame,
        };
        let gap = device_frame.saturating_sub(self.next_device_frame);
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
mod tests;
