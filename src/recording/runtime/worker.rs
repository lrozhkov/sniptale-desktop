//! Platform-neutral recording media pump.

use std::sync::Arc;

use crate::recording::encoder::samples::{BgraVideoSample, PcmAudioSample, SinkWriterSampleError};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct OwnedBgraVideoSample {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) stride: u32,
    pub(crate) bgra_bytes: Arc<[u8]>,
    pub(crate) timestamp_100ns: i64,
    pub(crate) duration_100ns: i64,
}

impl OwnedBgraVideoSample {
    pub(crate) fn as_borrowed(&self) -> BgraVideoSample<'_> {
        BgraVideoSample {
            width: self.width,
            height: self.height,
            stride: self.stride,
            bgra_bytes: self.bgra_bytes.as_ref(),
            timestamp_100ns: self.timestamp_100ns,
            duration_100ns: self.duration_100ns,
        }
    }

    fn compact_timestamp(&mut self, offset_100ns: i64) {
        self.timestamp_100ns = self.timestamp_100ns.saturating_sub(offset_100ns);
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct OwnedPcmAudioSample {
    pub(crate) bytes: Vec<u8>,
    pub(crate) timestamp_100ns: i64,
    pub(crate) duration_100ns: i64,
}

impl OwnedPcmAudioSample {
    pub(crate) fn as_borrowed(&self) -> PcmAudioSample<'_> {
        PcmAudioSample {
            bytes: &self.bytes,
            timestamp_100ns: self.timestamp_100ns,
            duration_100ns: self.duration_100ns,
        }
    }

    fn compact_timestamp(&mut self, offset_100ns: i64) {
        self.timestamp_100ns = self.timestamp_100ns.saturating_sub(offset_100ns);
    }
}

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct RecordingPumpProgress {
    pub(crate) video_samples: u32,
    pub(crate) audio_samples: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RecordingWorkerError {
    VideoSource,
    #[cfg(windows)]
    VideoPoll(i32),
    #[cfg(windows)]
    VideoTimestamp(i32),
    #[cfg(windows)]
    VideoSurface(i32),
    #[cfg(windows)]
    VideoInterface(i32),
    #[cfg(windows)]
    VideoReadback,
    #[cfg(windows)]
    VideoComposite,
    #[cfg(windows)]
    VideoGeometry,
    #[cfg(windows)]
    VideoClock,
    AudioSource,
    Sink(SinkWriterSampleError),
    SinkFailed,
    #[cfg(windows)]
    SinkNative(i32),
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum RecordingPumpState {
    #[default]
    Active,
    Paused,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum RecordingAudioPoll {
    Sample(OwnedPcmAudioSample),
    Pending,
    Drained,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum RecordingVideoPoll {
    Sample(OwnedBgraVideoSample),
    Pending,
}

pub(crate) trait RecordingVideoSource {
    fn next_video_sample(&mut self) -> Result<RecordingVideoPoll, RecordingWorkerError>;
}

pub(crate) trait RecordingAudioSource {
    fn next_audio_sample(&mut self) -> Result<RecordingAudioPoll, RecordingWorkerError>;
}

pub(crate) trait RecordingMediaSink {
    fn write_video_sample(
        &mut self,
        sample: BgraVideoSample<'_>,
    ) -> Result<(), RecordingWorkerError>;

    fn write_audio_sample(
        &mut self,
        sample: PcmAudioSample<'_>,
    ) -> Result<(), RecordingWorkerError>;
}

#[derive(Debug)]
pub(crate) struct RecordingMediaPump<V, A, S> {
    video: V,
    audio: A,
    sink: S,
    audio_drained: bool,
    pending_audio: Option<OwnedPcmAudioSample>,
    video_timeline_end_100ns: i64,
    video_pause_offset_100ns: i64,
    audio_pause_offset_100ns: i64,
}

// WASAPI commonly exposes several short capture packets between 30 fps video frames. Reading
// only one packet before blocking for the next frame lets the bounded endpoint buffer overflow
// and leaves the MP4 with only its first AAC frames. Keep the batch bounded so a malformed or
// synthetic source cannot starve video processing.
const MAX_AUDIO_SAMPLES_PER_TICK: u32 = 256;

impl<V, A, S> RecordingMediaPump<V, A, S> {
    pub(crate) const fn new(video: V, audio: A, sink: S) -> Self {
        Self {
            video,
            audio,
            sink,
            audio_drained: false,
            pending_audio: None,
            video_timeline_end_100ns: 0,
            video_pause_offset_100ns: 0,
            audio_pause_offset_100ns: 0,
        }
    }

    pub(crate) fn into_parts(self) -> (V, A, S) {
        (self.video, self.audio, self.sink)
    }
}

impl<V, A, S> RecordingMediaPump<V, A, S>
where
    V: RecordingVideoSource,
    A: RecordingAudioSource,
    S: RecordingMediaSink,
{
    #[cfg(test)]
    pub(crate) fn pump_once(&mut self) -> Result<RecordingPumpProgress, RecordingWorkerError> {
        self.pump_once_with_state(RecordingPumpState::Active)
    }

    pub(crate) fn pump_once_with_state(
        &mut self,
        state: RecordingPumpState,
    ) -> Result<RecordingPumpProgress, RecordingWorkerError> {
        let mut progress = RecordingPumpProgress::default();
        match self.video.next_video_sample()? {
            RecordingVideoPoll::Sample(mut sample) => {
                if state == RecordingPumpState::Paused {
                    self.video_pause_offset_100ns =
                        add_pause_offset(self.video_pause_offset_100ns, sample.duration_100ns)?;
                } else {
                    sample.compact_timestamp(self.video_pause_offset_100ns);
                    let sample_end = sample
                        .timestamp_100ns
                        .checked_add(sample.duration_100ns)
                        .ok_or(RecordingWorkerError::Sink(
                            SinkWriterSampleError::InvalidTime,
                        ))?;
                    self.sink.write_video_sample(sample.as_borrowed())?;
                    self.video_timeline_end_100ns = self.video_timeline_end_100ns.max(sample_end);
                    progress.video_samples = 1;
                }
            }
            RecordingVideoPoll::Pending => {}
        }
        progress.audio_samples = self.pump_ready_audio(state, MAX_AUDIO_SAMPLES_PER_TICK)?;
        Ok(progress)
    }

    fn pump_ready_audio(
        &mut self,
        state: RecordingPumpState,
        limit: u32,
    ) -> Result<u32, RecordingWorkerError> {
        let mut processed = 0_u32;
        let mut written = 0_u32;
        while !self.audio_drained && processed < limit {
            let poll = self.pending_audio.take().map_or_else(
                || self.audio.next_audio_sample(),
                |sample| Ok(RecordingAudioPoll::Sample(sample)),
            )?;
            match poll {
                RecordingAudioPoll::Sample(mut sample) => {
                    processed = processed.saturating_add(1);
                    if state == RecordingPumpState::Paused {
                        self.audio_pause_offset_100ns =
                            add_pause_offset(self.audio_pause_offset_100ns, sample.duration_100ns)?;
                    } else {
                        let compact_timestamp = sample
                            .timestamp_100ns
                            .saturating_sub(self.audio_pause_offset_100ns);
                        let sample_end =
                            compact_timestamp.checked_add(sample.duration_100ns).ok_or(
                                RecordingWorkerError::Sink(SinkWriterSampleError::InvalidTime),
                            )?;
                        if sample_end > self.video_timeline_end_100ns {
                            self.pending_audio = Some(sample);
                            break;
                        }
                        sample.compact_timestamp(self.audio_pause_offset_100ns);
                        self.sink.write_audio_sample(sample.as_borrowed())?;
                        written = written.saturating_add(1);
                    }
                }
                RecordingAudioPoll::Pending => break,
                RecordingAudioPoll::Drained => self.audio_drained = true,
            }
        }
        Ok(written)
    }
}

fn add_pause_offset(offset_100ns: i64, duration_100ns: i64) -> Result<i64, RecordingWorkerError> {
    if duration_100ns <= 0 {
        return Err(RecordingWorkerError::Sink(
            SinkWriterSampleError::InvalidTime,
        ));
    }
    offset_100ns
        .checked_add(duration_100ns)
        .ok_or(RecordingWorkerError::Sink(
            SinkWriterSampleError::InvalidTime,
        ))
}
