use super::*;

#[derive(Debug)]
struct FakeVideoSource {
    samples: Vec<OwnedBgraVideoSample>,
    cursor: usize,
    fail: bool,
}

impl RecordingVideoSource for FakeVideoSource {
    fn next_video_sample(&mut self) -> Result<RecordingVideoPoll, RecordingWorkerError> {
        if self.fail {
            return Err(RecordingWorkerError::VideoSource);
        }
        let sample = self.samples.get(self.cursor).cloned();
        if sample.is_some() {
            self.cursor = self.cursor.saturating_add(1);
        }
        Ok(match sample {
            Some(sample) => RecordingVideoPoll::Sample(sample),
            None => RecordingVideoPoll::Pending,
        })
    }
}

#[derive(Debug)]
struct FakeAudioSource {
    polls_to_return: Vec<RecordingAudioPoll>,
    cursor: usize,
    polls: u32,
}

impl RecordingAudioSource for FakeAudioSource {
    fn next_audio_sample(&mut self) -> Result<RecordingAudioPoll, RecordingWorkerError> {
        self.polls = self.polls.saturating_add(1);
        let poll = self
            .polls_to_return
            .get(self.cursor)
            .cloned()
            .unwrap_or(RecordingAudioPoll::Drained);
        if self.cursor < self.polls_to_return.len() {
            self.cursor = self.cursor.saturating_add(1);
        }
        Ok(poll)
    }
}

#[derive(Debug, Default)]
struct FakeSink {
    events: Vec<&'static str>,
    video_timestamps: Vec<i64>,
    audio_timestamps: Vec<i64>,
    fail_audio: bool,
}

impl RecordingMediaSink for FakeSink {
    fn write_video_sample(
        &mut self,
        sample: BgraVideoSample<'_>,
    ) -> Result<(), RecordingWorkerError> {
        if sample.timestamp_100ns == 0 {
            self.events.push("video-0");
        } else {
            self.events.push("video-1");
        }
        self.video_timestamps.push(sample.timestamp_100ns);
        Ok(())
    }

    fn write_audio_sample(
        &mut self,
        sample: PcmAudioSample<'_>,
    ) -> Result<(), RecordingWorkerError> {
        if self.fail_audio {
            return Err(RecordingWorkerError::SinkFailed);
        }
        self.events.push("audio");
        self.audio_timestamps.push(sample.timestamp_100ns);
        Ok(())
    }
}

#[test]
fn media_pump_writes_available_video_and_audio_samples() {
    let video = FakeVideoSource {
        samples: vec![video_sample(0), video_sample(333_333)],
        cursor: 0,
        fail: false,
    };
    let audio = FakeAudioSource {
        polls_to_return: vec![
            RecordingAudioPoll::Sample(audio_sample()),
            RecordingAudioPoll::Drained,
        ],
        cursor: 0,
        polls: 0,
    };
    let mut pump = RecordingMediaPump::new(video, audio, FakeSink::default());

    assert_eq!(
        pump.pump_once(),
        Ok(RecordingPumpProgress {
            video_samples: 1,
            audio_samples: 1,
        })
    );
    assert_eq!(
        pump.pump_once(),
        Ok(RecordingPumpProgress {
            video_samples: 1,
            audio_samples: 0,
        })
    );
    assert_eq!(
        pump.pump_once(),
        Ok(RecordingPumpProgress {
            video_samples: 0,
            audio_samples: 0,
        })
    );

    let (_video, audio, sink) = pump.into_parts();
    assert_eq!(audio.polls, 2);
    assert_eq!(sink.events, ["video-0", "audio", "video-1"]);
}

#[test]
fn media_pump_does_not_poll_audio_after_video_source_failure() {
    let video = FakeVideoSource {
        samples: Vec::new(),
        cursor: 0,
        fail: true,
    };
    let audio = FakeAudioSource {
        polls_to_return: vec![RecordingAudioPoll::Sample(audio_sample())],
        cursor: 0,
        polls: 0,
    };
    let mut pump = RecordingMediaPump::new(video, audio, FakeSink::default());

    assert_eq!(pump.pump_once(), Err(RecordingWorkerError::VideoSource));

    let (_video, audio, sink) = pump.into_parts();
    assert_eq!(audio.polls, 0);
    assert!(sink.events.is_empty());
}

#[test]
fn media_pump_propagates_sink_failure() {
    let video = FakeVideoSource {
        samples: vec![video_sample(0)],
        cursor: 0,
        fail: false,
    };
    let audio = FakeAudioSource {
        polls_to_return: vec![RecordingAudioPoll::Sample(audio_sample())],
        cursor: 0,
        polls: 0,
    };
    let mut pump = RecordingMediaPump::new(
        video,
        audio,
        FakeSink {
            events: Vec::new(),
            video_timestamps: Vec::new(),
            audio_timestamps: Vec::new(),
            fail_audio: true,
        },
    );

    assert_eq!(pump.pump_once(), Err(RecordingWorkerError::SinkFailed));

    let (_video, audio, sink) = pump.into_parts();
    assert_eq!(audio.polls, 1);
    assert_eq!(sink.events, ["video-0"]);
}

#[test]
fn media_pump_keeps_polling_audio_after_pending_tick() {
    let video = FakeVideoSource {
        samples: vec![video_sample(0)],
        cursor: 0,
        fail: false,
    };
    let audio = FakeAudioSource {
        polls_to_return: vec![
            RecordingAudioPoll::Pending,
            RecordingAudioPoll::Sample(audio_sample()),
            RecordingAudioPoll::Drained,
        ],
        cursor: 0,
        polls: 0,
    };
    let mut pump = RecordingMediaPump::new(video, audio, FakeSink::default());

    assert_eq!(
        pump.pump_once(),
        Ok(RecordingPumpProgress {
            video_samples: 1,
            audio_samples: 0,
        })
    );
    assert_eq!(
        pump.pump_once(),
        Ok(RecordingPumpProgress {
            video_samples: 0,
            audio_samples: 1,
        })
    );
    assert_eq!(
        pump.pump_once(),
        Ok(RecordingPumpProgress {
            video_samples: 0,
            audio_samples: 0,
        })
    );

    let (_video, audio, sink) = pump.into_parts();
    assert_eq!(audio.polls, 3);
    assert_eq!(sink.events, ["video-0", "audio"]);
}

#[test]
fn media_pump_drains_ready_audio_while_video_frame_is_pending() {
    let video = FakeVideoSource {
        samples: vec![video_sample(0)],
        cursor: 0,
        fail: false,
    };
    let audio = FakeAudioSource {
        polls_to_return: vec![
            RecordingAudioPoll::Pending,
            RecordingAudioPoll::Sample(audio_sample()),
            RecordingAudioPoll::Pending,
        ],
        cursor: 0,
        polls: 0,
    };
    let mut pump = RecordingMediaPump::new(video, audio, FakeSink::default());

    assert_eq!(
        pump.pump_once(),
        Ok(RecordingPumpProgress {
            video_samples: 1,
            audio_samples: 0,
        })
    );

    assert_eq!(
        pump.pump_once(),
        Ok(RecordingPumpProgress {
            video_samples: 0,
            audio_samples: 1,
        })
    );

    let (_video, audio, sink) = pump.into_parts();
    assert_eq!(audio.polls, 3);
    assert_eq!(sink.events, ["video-0", "audio"]);
}

#[test]
fn media_pump_holds_audio_ahead_of_video_until_the_video_timeline_advances() {
    let video = FakeVideoSource {
        samples: vec![video_sample(0), video_sample(333_333)],
        cursor: 0,
        fail: false,
    };
    let audio = FakeAudioSource {
        polls_to_return: vec![
            RecordingAudioPoll::Sample(audio_sample_at(400_000, 100_000)),
            RecordingAudioPoll::Drained,
        ],
        cursor: 0,
        polls: 0,
    };
    let mut pump = RecordingMediaPump::new(video, audio, FakeSink::default());

    assert_eq!(
        pump.pump_once(),
        Ok(RecordingPumpProgress {
            video_samples: 1,
            audio_samples: 0,
        })
    );
    assert_eq!(
        pump.pump_once(),
        Ok(RecordingPumpProgress {
            video_samples: 1,
            audio_samples: 1,
        })
    );

    let (_video, audio, sink) = pump.into_parts();
    assert_eq!(audio.polls, 2);
    assert_eq!(sink.events, ["video-0", "video-1", "audio"]);
}

#[test]
fn media_pump_drains_all_ready_audio_before_waiting_for_another_video_frame() {
    let video = FakeVideoSource {
        samples: vec![video_sample(0)],
        cursor: 0,
        fail: false,
    };
    let audio = FakeAudioSource {
        polls_to_return: vec![
            RecordingAudioPoll::Sample(audio_sample_at(0, 100_000)),
            RecordingAudioPoll::Sample(audio_sample_at(100_000, 100_000)),
            RecordingAudioPoll::Sample(audio_sample_at(200_000, 100_000)),
            RecordingAudioPoll::Pending,
        ],
        cursor: 0,
        polls: 0,
    };
    let mut pump = RecordingMediaPump::new(video, audio, FakeSink::default());

    assert_eq!(
        pump.pump_once(),
        Ok(RecordingPumpProgress {
            video_samples: 1,
            audio_samples: 3,
        })
    );

    let (_video, audio, sink) = pump.into_parts();
    assert_eq!(audio.polls, 4);
    assert_eq!(sink.events, ["video-0", "audio", "audio", "audio"]);
    assert_eq!(sink.audio_timestamps, [0, 100_000, 200_000]);
}

#[test]
fn media_pump_discards_samples_while_paused_and_compacts_timestamps() {
    let video = FakeVideoSource {
        samples: vec![video_sample(0), video_sample(333_333)],
        cursor: 0,
        fail: false,
    };
    let audio = FakeAudioSource {
        polls_to_return: vec![
            RecordingAudioPoll::Sample(audio_sample_at(0, 100_000)),
            RecordingAudioPoll::Pending,
            RecordingAudioPoll::Sample(audio_sample_at(100_000, 100_000)),
            RecordingAudioPoll::Pending,
        ],
        cursor: 0,
        polls: 0,
    };
    let mut pump = RecordingMediaPump::new(video, audio, FakeSink::default());

    assert_eq!(
        pump.pump_once_with_state(RecordingPumpState::Paused),
        Ok(RecordingPumpProgress {
            video_samples: 0,
            audio_samples: 0,
        })
    );
    assert_eq!(
        pump.pump_once_with_state(RecordingPumpState::Active),
        Ok(RecordingPumpProgress {
            video_samples: 1,
            audio_samples: 1,
        })
    );

    let (_video, audio, sink) = pump.into_parts();
    assert_eq!(audio.polls, 4);
    assert_eq!(sink.events, ["video-0", "audio"]);
    assert_eq!(sink.video_timestamps, [0]);
    assert_eq!(sink.audio_timestamps, [0]);
}

fn video_sample(timestamp_100ns: i64) -> OwnedBgraVideoSample {
    OwnedBgraVideoSample {
        width: 1,
        height: 1,
        stride: 4,
        bgra_bytes: vec![0, 0, 0, 0].into(),
        timestamp_100ns,
        duration_100ns: 333_333,
    }
}

fn audio_sample() -> OwnedPcmAudioSample {
    audio_sample_at(0, 100_000)
}

fn audio_sample_at(timestamp_100ns: i64, duration_100ns: i64) -> OwnedPcmAudioSample {
    OwnedPcmAudioSample {
        bytes: vec![0, 0, 0, 0],
        timestamp_100ns,
        duration_100ns,
    }
}
