//! One validated, normalized packet per input; pending peers never consume this packet.

use super::{OwnedPcmInputChunk, PcmChunkSource, PcmSourceError, PcmSourcePoll};
use crate::audio::{
    mixer::{AudioInputKind, MixPlan, PcmFormat, validate_format},
    pcm_mix::{MixedPcmChunk, PcmInputChunk, PcmMixError, mix_pcm_chunks},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum BufferedPoll {
    Ready,
    Pending,
    Drained,
}

#[derive(Debug)]
pub(super) struct InputBuffer<T> {
    pub(super) source: Option<T>,
    pub(super) packet: Option<BufferedPacket>,
    next_input_timestamp: Option<i64>,
    ended: bool,
}

#[derive(Debug)]
pub(super) struct BufferedPacket {
    chunk: MixedPcmChunk,
    offset: usize,
}

impl BufferedPacket {
    pub(super) const fn timestamp(&self) -> i64 {
        self.chunk.timestamp_100ns
    }

    pub(super) fn remaining_bytes(&self) -> usize {
        self.chunk.bytes.len() - self.offset
    }

    pub(super) fn prefix(
        &self,
        count: usize,
        kind: AudioInputKind,
        format: PcmFormat,
        timestamp_100ns: i64,
        duration_100ns: i64,
    ) -> Result<PcmInputChunk<'_>, PcmMixError> {
        let end = self
            .offset
            .checked_add(count)
            .ok_or(PcmMixError::OutputTooLarge)?;
        let bytes = self
            .chunk
            .bytes
            .get(self.offset..end)
            .ok_or(PcmMixError::InvalidInputBytes)?;
        Ok(PcmInputChunk {
            kind,
            format,
            bytes,
            timestamp_100ns,
            duration_100ns,
        })
    }
}

impl<T> InputBuffer<T> {
    pub(super) const fn new(source: Option<T>) -> Self {
        Self {
            source,
            packet: None,
            next_input_timestamp: None,
            ended: false,
        }
    }

    pub(super) fn take_packet(&mut self) -> Option<MixedPcmChunk> {
        self.packet.take().map(|packet| packet.chunk)
    }

    pub(super) fn consume(&mut self, count: usize) -> Result<(), PcmSourceError> {
        let packet = self
            .packet
            .as_mut()
            .ok_or(PcmSourceError::Mix(PcmMixError::InvalidInputBytes))?;
        let next = packet
            .offset
            .checked_add(count)
            .filter(|next| *next <= packet.chunk.bytes.len())
            .ok_or(PcmSourceError::Mix(PcmMixError::InvalidInputBytes))?;
        packet.offset = next;
        if packet.remaining_bytes() == 0 {
            self.packet = None;
        }
        Ok(())
    }
}

impl<T: PcmChunkSource> InputBuffer<T> {
    pub(super) fn poll(
        &mut self,
        kind: AudioInputKind,
        plan: &MixPlan,
    ) -> Result<BufferedPoll, PcmSourceError> {
        let required = match kind {
            AudioInputKind::Microphone => plan.microphone.is_some(),
            AudioInputKind::SystemLoopback => plan.system_loopback.is_some(),
        };
        if !required || self.ended {
            return Ok(BufferedPoll::Drained);
        }
        if self.packet.is_some() {
            return Ok(BufferedPoll::Ready);
        }
        let source = self
            .source
            .as_mut()
            .ok_or(PcmSourceError::SourceEndedEarly(kind))?;
        match source.next_pcm_chunk()? {
            PcmSourcePoll::Pending => Ok(BufferedPoll::Pending),
            PcmSourcePoll::Drained => {
                self.ended = true;
                Ok(BufferedPoll::Drained)
            }
            PcmSourcePoll::Chunk(chunk) => {
                validate_packet_budget(&chunk).map_err(PcmSourceError::Mix)?;
                if self
                    .next_input_timestamp
                    .is_some_and(|expected| expected != chunk.timestamp_100ns)
                {
                    return Err(PcmSourceError::Mix(PcmMixError::UnsyncedInputs));
                }
                let next = chunk
                    .timestamp_100ns
                    .checked_add(chunk.duration_100ns)
                    .ok_or(PcmSourceError::Mix(PcmMixError::InvalidInputTime))?;
                let single_plan = MixPlan {
                    target_format: plan.target_format,
                    microphone: if kind == AudioInputKind::Microphone {
                        plan.microphone
                    } else {
                        None
                    },
                    system_loopback: if kind == AudioInputKind::SystemLoopback {
                        plan.system_loopback
                    } else {
                        None
                    },
                };
                let normalized = match kind {
                    AudioInputKind::Microphone => {
                        mix_pcm_chunks(&single_plan, Some(chunk.as_input()), None)
                    }
                    AudioInputKind::SystemLoopback => {
                        mix_pcm_chunks(&single_plan, None, Some(chunk.as_input()))
                    }
                }
                .map_err(PcmSourceError::Mix)?;
                self.next_input_timestamp = Some(next);
                self.packet = Some(BufferedPacket {
                    chunk: normalized,
                    offset: 0,
                });
                Ok(BufferedPoll::Ready)
            }
        }
    }
}

fn validate_packet_budget(chunk: &OwnedPcmInputChunk) -> Result<(), PcmMixError> {
    validate_format(chunk.format).map_err(|_| PcmMixError::InvalidFormat)?;
    // Retain at most two seconds per input, even for a malformed source. Normalize only after
    // this limit, so resampling cannot allocate an unbounded expanded packet.
    let maximum = u64::from(chunk.format.sample_rate_hz) * u64::from(chunk.format.channels) * 2 * 2;
    if u64::try_from(chunk.bytes.len()).map_err(|_| PcmMixError::OutputTooLarge)? > maximum {
        return Err(PcmMixError::OutputTooLarge);
    }
    Ok(())
}
