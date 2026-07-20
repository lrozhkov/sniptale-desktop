//! Platform-neutral PCM source coordination before encoding.
use super::{
    mixer::{AudioInputKind, MixPlan, PcmFormat},
    pcm_mix::{MixedPcmChunk, PcmInputChunk, PcmMixError, mix_pcm_chunks},
};

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
    microphone: Option<M>,
    system_loopback: Option<S>,
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
            microphone,
            system_loopback,
            drained: false,
        }
    }

    #[cfg(test)]
    pub(crate) fn into_parts(self) -> (Option<M>, Option<S>) {
        (self.microphone, self.system_loopback)
    }
}

impl<M, S> MixedPcmSource for PcmMixPipeline<M, S>
where
    M: PcmChunkSource,
    S: PcmChunkSource,
{
    fn next_mixed_chunk(&mut self) -> Result<MixedPcmPoll, PcmSourceError> {
        if self.drained {
            return Ok(MixedPcmPoll::Drained);
        }

        let microphone = poll_required_source(
            self.plan.microphone.is_some(),
            self.microphone.as_mut(),
            AudioInputKind::Microphone,
        )?;
        let system_loopback = poll_required_source(
            self.plan.system_loopback.is_some(),
            self.system_loopback.as_mut(),
            AudioInputKind::SystemLoopback,
        )?;

        if matches!(microphone, RequiredPcmPoll::Pending)
            || matches!(system_loopback, RequiredPcmPoll::Pending)
        {
            return Ok(MixedPcmPoll::Pending);
        }
        if matches!(microphone, RequiredPcmPoll::Drained)
            && matches!(system_loopback, RequiredPcmPoll::Drained)
        {
            self.drained = true;
            return Ok(MixedPcmPoll::Drained);
        }
        let microphone = microphone.into_chunk();
        let system_loopback = system_loopback.into_chunk();
        require_synced_presence(&self.plan, microphone.as_ref(), system_loopback.as_ref())?;

        mix_pcm_chunks(
            &self.plan,
            microphone.as_ref().map(OwnedPcmInputChunk::as_input),
            system_loopback.as_ref().map(OwnedPcmInputChunk::as_input),
        )
        .map(MixedPcmPoll::Chunk)
        .map_err(PcmSourceError::Mix)
    }
}

#[derive(Debug)]
enum RequiredPcmPoll {
    Chunk(OwnedPcmInputChunk),
    Pending,
    Drained,
}

impl RequiredPcmPoll {
    fn into_chunk(self) -> Option<OwnedPcmInputChunk> {
        match self {
            Self::Chunk(chunk) => Some(chunk),
            Self::Pending | Self::Drained => None,
        }
    }
}

fn poll_required_source<T>(
    required: bool,
    source: Option<&mut T>,
    kind: AudioInputKind,
) -> Result<RequiredPcmPoll, PcmSourceError>
where
    T: PcmChunkSource,
{
    if !required {
        return Ok(RequiredPcmPoll::Drained);
    }
    let poll = source
        .ok_or(PcmSourceError::SourceEndedEarly(kind))?
        .next_pcm_chunk()?;
    Ok(match poll {
        PcmSourcePoll::Chunk(chunk) => RequiredPcmPoll::Chunk(chunk),
        PcmSourcePoll::Pending => RequiredPcmPoll::Pending,
        PcmSourcePoll::Drained => RequiredPcmPoll::Drained,
    })
}

fn require_synced_presence(
    plan: &MixPlan,
    microphone: Option<&OwnedPcmInputChunk>,
    system_loopback: Option<&OwnedPcmInputChunk>,
) -> Result<(), PcmSourceError> {
    if plan.microphone.is_some() && microphone.is_none() {
        return Err(PcmSourceError::SourceEndedEarly(AudioInputKind::Microphone));
    }
    if plan.system_loopback.is_some() && system_loopback.is_none() {
        return Err(PcmSourceError::SourceEndedEarly(
            AudioInputKind::SystemLoopback,
        ));
    }
    Ok(())
}
