//! Safe packet-header validation and bounded pending PCM construction.

use super::{WindowsWasapiPcmError, WindowsWasapiPcmSource, format::timestamp_error_flag};
use crate::audio::pcm_source::OwnedPcmInputChunk;

pub(super) fn packet_bytes(
    frames: u32,
    flags: u32,
    buffer_frames: u32,
    block_align: usize,
) -> Result<usize, WindowsWasapiPcmError> {
    if frames > buffer_frames {
        return Err(WindowsWasapiPcmError::BufferTooLarge);
    }
    if flags & timestamp_error_flag() != 0 {
        return Err(WindowsWasapiPcmError::InvalidPacketTimeline);
    }
    usize::try_from(frames)
        .ok()
        .and_then(|frames| frames.checked_mul(block_align))
        .ok_or(WindowsWasapiPcmError::BufferTooLarge)
}

impl WindowsWasapiPcmSource {
    pub(super) fn emit_pending_packet(
        &mut self,
    ) -> Result<Option<OwnedPcmInputChunk>, WindowsWasapiPcmError> {
        let Some(emission) = self
            .timeline
            .next_emission()
            .map_err(|_| WindowsWasapiPcmError::InvalidPacketTimeline)?
        else {
            return Ok(None);
        };
        let bytes = if emission.silence {
            let length = usize::try_from(emission.frames)
                .ok()
                .and_then(|frames| frames.checked_mul(self.block_align))
                .ok_or(WindowsWasapiPcmError::BufferTooLarge)?;
            vec![0; length]
        } else {
            self.pending_packet
                .take()
                .ok_or(WindowsWasapiPcmError::InvalidPacketTimeline)?
        };
        Ok(Some(OwnedPcmInputChunk {
            kind: self.kind,
            format: self.format,
            bytes,
            timestamp_100ns: emission.timestamp_100ns,
            duration_100ns: emission.duration_100ns,
        }))
    }
}
