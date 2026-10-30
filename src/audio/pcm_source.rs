//! Platform-neutral PCM source coordination before encoding.

mod buffer;

use super::{
    mixer::{AudioInputKind, MixPlan, PcmFormat},
    pcm_mix::{MixedPcmChunk, PcmInputChunk, PcmMixError, mix_pcm_chunks},
};
use buffer::{BufferedPacket, BufferedPoll, InputBuffer};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct OwnedPcmInputChunk {
    pub(crate) kind: AudioInputKind,
    pub(crate) format: PcmFormat,
    pub(crate) bytes: Vec<u8>,
    pub(crate) timestamp_100ns: i64,
    pub(crate) duration_100ns: i64,
}

impl OwnedPcmInputChunk {
    pub(crate) fn as_input(&self) -> PcmInputChunk<'_> {
        PcmInputChunk {
            kind: self.kind,
            format: self.format,
            bytes: &self.bytes,
            timestamp_100ns: self.timestamp_100ns,
            duration_100ns: self.duration_100ns,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PcmSourceError {
    Source(AudioInputKind),
    SourceEndedEarly(AudioInputKind),
    Mix(PcmMixError),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum PcmSourcePoll {
    Chunk(OwnedPcmInputChunk),
    Pending,
    Drained,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum MixedPcmPoll {
    Chunk(MixedPcmChunk),
    Pending,
    Drained,
}

pub(crate) trait PcmChunkSource {
    fn next_pcm_chunk(&mut self) -> Result<PcmSourcePoll, PcmSourceError>;
}

pub(crate) trait MixedPcmSource {
    fn next_mixed_chunk(&mut self) -> Result<MixedPcmPoll, PcmSourceError>;
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct NoPcmChunkSource;

impl PcmChunkSource for NoPcmChunkSource {
    fn next_pcm_chunk(&mut self) -> Result<PcmSourcePoll, PcmSourceError> {
        Ok(PcmSourcePoll::Drained)
    }
}

#[derive(Debug)]
pub(crate) struct PcmMixPipeline<M = NoPcmChunkSource, S = NoPcmChunkSource> {
    plan: MixPlan,
    microphone: InputBuffer<M>,
    system_loopback: InputBuffer<S>,
    epoch: Option<i64>,
    emitted_frames: u64,
    drained: bool,
}

impl<M, S> PcmMixPipeline<M, S> {
    pub(crate) const fn new(
        plan: MixPlan,
        microphone: Option<M>,
        system_loopback: Option<S>,
    ) -> Self {
        Self {
            plan,
            microphone: InputBuffer::new(microphone),
            system_loopback: InputBuffer::new(system_loopback),
            epoch: None,
            emitted_frames: 0,
            drained: false,
        }
    }

    #[cfg(test)]
    pub(crate) fn into_parts(self) -> (Option<M>, Option<S>) {
        (self.microphone.source, self.system_loopback.source)
    }
}

impl<M: PcmChunkSource, S: PcmChunkSource> MixedPcmSource for PcmMixPipeline<M, S> {
    fn next_mixed_chunk(&mut self) -> Result<MixedPcmPoll, PcmSourceError> {
        if self.drained {
            return Ok(MixedPcmPoll::Drained);
        }
        let microphone = self
            .microphone
            .poll(AudioInputKind::Microphone, &self.plan)?;
        let system = self
            .system_loopback
            .poll(AudioInputKind::SystemLoopback, &self.plan)?;
        if microphone == BufferedPoll::Pending || system == BufferedPoll::Pending {
            return Ok(MixedPcmPoll::Pending);
        }
        if microphone == BufferedPoll::Drained && system == BufferedPoll::Drained {
            self.drained = true;
            return Ok(MixedPcmPoll::Drained);
        }
        if self.plan.microphone.is_none() {
            return self
                .system_loopback
                .take_packet()
                .map(MixedPcmPoll::Chunk)
                .ok_or(PcmSourceError::SourceEndedEarly(
                    AudioInputKind::SystemLoopback,
                ));
        }
        if self.plan.system_loopback.is_none() {
            return self
                .microphone
                .take_packet()
                .map(MixedPcmPoll::Chunk)
                .ok_or(PcmSourceError::SourceEndedEarly(AudioInputKind::Microphone));
        }
        self.mix_prefix().map(MixedPcmPoll::Chunk)
    }
}

impl<M, S> PcmMixPipeline<M, S> {
    fn mix_prefix(&mut self) -> Result<MixedPcmChunk, PcmSourceError> {
        let microphone = self
            .microphone
            .packet
            .as_ref()
            .ok_or(PcmSourceError::SourceEndedEarly(AudioInputKind::Microphone))?;
        let system =
            self.system_loopback
                .packet
                .as_ref()
                .ok_or(PcmSourceError::SourceEndedEarly(
                    AudioInputKind::SystemLoopback,
                ))?;
        let prefix = prefix_plan(
            microphone,
            system,
            self.plan.target_format,
            self.epoch,
            self.emitted_frames,
        )?;
        // WASAPI inputs have independent packet boundaries. Normalize each once, mix their
        // common frame prefix, and retain the longer packet for the next peer packet. The
        // output timeline is derived from cumulative frames, avoiding per-packet rounding drift.
        let mixed = prefix.mix(&self.plan, microphone, system)?;
        self.microphone.consume(prefix.bytes)?;
        self.system_loopback.consume(prefix.bytes)?;
        self.epoch = Some(prefix.epoch);
        self.emitted_frames = prefix.end_frame;
        Ok(mixed)
    }
}

struct PcmPrefixPlan {
    epoch: i64,
    start: i64,
    duration: i64,
    bytes: usize,
    end_frame: u64,
}

impl PcmPrefixPlan {
    fn mix(
        &self,
        plan: &MixPlan,
        microphone: &BufferedPacket,
        system: &BufferedPacket,
    ) -> Result<MixedPcmChunk, PcmSourceError> {
        mix_pcm_chunks(
            plan,
            Some(
                microphone
                    .prefix(
                        self.bytes,
                        AudioInputKind::Microphone,
                        plan.target_format,
                        self.start,
                        self.duration,
                    )
                    .map_err(PcmSourceError::Mix)?,
            ),
            Some(
                system
                    .prefix(
                        self.bytes,
                        AudioInputKind::SystemLoopback,
                        plan.target_format,
                        self.start,
                        self.duration,
                    )
                    .map_err(PcmSourceError::Mix)?,
            ),
        )
        .map_err(PcmSourceError::Mix)
    }
}

fn prefix_plan(
    microphone: &BufferedPacket,
    system: &BufferedPacket,
    format: PcmFormat,
    epoch: Option<i64>,
    emitted_frames: u64,
) -> Result<PcmPrefixPlan, PcmSourceError> {
    let epoch = match epoch {
        Some(epoch) => epoch,
        None if microphone.timestamp() == system.timestamp() => microphone.timestamp(),
        None => return Err(PcmSourceError::Mix(PcmMixError::UnsyncedInputs)),
    };
    let frame_bytes = usize::from(format.channels) * 2;
    let bytes = microphone.remaining_bytes().min(system.remaining_bytes());
    let frames = u64::try_from(bytes / frame_bytes)
        .map_err(|_| PcmSourceError::Mix(PcmMixError::OutputTooLarge))?;
    let end_frame = emitted_frames
        .checked_add(frames)
        .ok_or(PcmSourceError::Mix(PcmMixError::OutputTooLarge))?;
    let start = frame_time(emitted_frames, format.sample_rate_hz, epoch)?;
    let end = frame_time(end_frame, format.sample_rate_hz, epoch)?;
    let duration = end
        .checked_sub(start)
        .filter(|duration| *duration > 0)
        .ok_or(PcmSourceError::Mix(PcmMixError::InvalidInputTime))?;
    Ok(PcmPrefixPlan {
        epoch,
        start,
        duration,
        bytes,
        end_frame,
    })
}

fn frame_time(frames: u64, rate: u32, epoch: i64) -> Result<i64, PcmSourceError> {
    let elapsed = u128::from(frames)
        .checked_mul(10_000_000)
        .and_then(|time| time.checked_div(u128::from(rate)))
        .and_then(|time| i64::try_from(time).ok())
        .and_then(|time| epoch.checked_add(time))
        .ok_or(PcmSourceError::Mix(PcmMixError::InvalidInputTime))?;
    Ok(elapsed)
}
