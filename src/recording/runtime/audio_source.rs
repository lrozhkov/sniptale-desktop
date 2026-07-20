//! Recording worker adapter for mixed PCM audio sources.

use crate::audio::{MixedPcmPoll, MixedPcmSource, PcmSourceError};

use super::worker::{
    OwnedPcmAudioSample, RecordingAudioPoll, RecordingAudioSource, RecordingWorkerError,
};

#[derive(Debug)]
pub(crate) struct RecordingPcmAudioSource<A> {
    source: A,
}

impl<A> RecordingPcmAudioSource<A> {
    pub(crate) const fn new(source: A) -> Self {
        Self { source }
    }
}

impl<A> RecordingAudioSource for RecordingPcmAudioSource<A>
where
    A: MixedPcmSource,
{
    fn next_audio_sample(&mut self) -> Result<RecordingAudioPoll, RecordingWorkerError> {
        self.source
            .next_mixed_chunk()
            .map(|chunk| match chunk {
                MixedPcmPoll::Chunk(chunk) => RecordingAudioPoll::Sample(OwnedPcmAudioSample {
                    bytes: chunk.bytes,
                    timestamp_100ns: chunk.timestamp_100ns,
                    duration_100ns: chunk.duration_100ns,
                }),
                MixedPcmPoll::Pending => RecordingAudioPoll::Pending,
                MixedPcmPoll::Drained => RecordingAudioPoll::Drained,
            })
            .map_err(worker_error)
    }
}

const fn worker_error(_error: PcmSourceError) -> RecordingWorkerError {
    RecordingWorkerError::AudioSource
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use crate::audio::{
        AudioInputKind, MixedPcmChunk, MixedPcmPoll, MixedPcmSource, PcmMixError, PcmSourceError,
    };

    use super::*;

    #[test]
    fn recording_pcm_audio_source_maps_mixed_chunks_to_worker_samples() {
        let source = FakeMixedPcmSource::new(vec![Ok(MixedPcmPoll::Chunk(MixedPcmChunk {
            bytes: vec![1, 2, 3, 4],
            timestamp_100ns: 10,
            duration_100ns: 20,
        }))]);
        let mut source = RecordingPcmAudioSource::new(source);

        assert_eq!(
            source.next_audio_sample(),
            Ok(RecordingAudioPoll::Sample(OwnedPcmAudioSample {
                bytes: vec![1, 2, 3, 4],
                timestamp_100ns: 10,
                duration_100ns: 20,
            }))
        );
    }

    #[test]
    fn recording_pcm_audio_source_propagates_drain() {
        let mut source =
            RecordingPcmAudioSource::new(FakeMixedPcmSource::new(vec![Ok(MixedPcmPoll::Drained)]));

        assert_eq!(source.next_audio_sample(), Ok(RecordingAudioPoll::Drained));
    }

    #[test]
    fn recording_pcm_audio_source_preserves_pending_state() {
        let mut source =
            RecordingPcmAudioSource::new(FakeMixedPcmSource::new(vec![Ok(MixedPcmPoll::Pending)]));

        assert_eq!(source.next_audio_sample(), Ok(RecordingAudioPoll::Pending));
    }

    #[test]
    fn recording_pcm_audio_source_maps_source_and_mix_errors_to_audio_source_failure() {
        let mut source = RecordingPcmAudioSource::new(FakeMixedPcmSource::new(vec![Err(
            PcmSourceError::Source(AudioInputKind::Microphone),
        )]));
        assert_eq!(
            source.next_audio_sample(),
            Err(RecordingWorkerError::AudioSource)
        );

        let mut source = RecordingPcmAudioSource::new(FakeMixedPcmSource::new(vec![Err(
            PcmSourceError::Mix(PcmMixError::InvalidInputBytes),
        )]));
        assert_eq!(
            source.next_audio_sample(),
            Err(RecordingWorkerError::AudioSource)
        );
    }

    #[derive(Debug)]
    struct FakeMixedPcmSource {
        chunks: VecDeque<Result<MixedPcmPoll, PcmSourceError>>,
    }

    impl FakeMixedPcmSource {
        fn new(chunks: Vec<Result<MixedPcmPoll, PcmSourceError>>) -> Self {
            Self {
                chunks: VecDeque::from(chunks),
            }
        }
    }

    impl MixedPcmSource for FakeMixedPcmSource {
        fn next_mixed_chunk(&mut self) -> Result<MixedPcmPoll, PcmSourceError> {
            self.chunks.pop_front().unwrap_or(Ok(MixedPcmPoll::Drained))
        }
    }
}
