//! Windows WGC recording video source adapter.
#![cfg(windows)]

use std::{sync::Arc, time::Instant};

use crate::{
    capture::{WgcRecordingFrameError, WindowsWgcRecordingSession},
    protocol::NativeRecordingSource,
    recording::{
        encoder::H264SinkWriterStreamPlan,
        runtime::{
            frame_clock::VideoFrameClock,
            worker::{
                OwnedBgraVideoSample, RecordingVideoPoll, RecordingVideoSource,
                RecordingWorkerError,
            },
        },
    },
};

#[derive(Clone, Debug)]
struct CachedBgraFrame {
    width: u32,
    height: u32,
    stride: u32,
    bgra_bytes: Arc<[u8]>,
}

#[derive(Debug)]
pub(crate) struct WindowsWgcRecordingVideoSource {
    session: WindowsWgcRecordingSession,
    plan: H264SinkWriterStreamPlan,
    clock: VideoFrameClock,
    wall_epoch: Option<Instant>,
    last_frame: Option<CachedBgraFrame>,
}

impl WindowsWgcRecordingVideoSource {
    pub(crate) fn start(
        source: &NativeRecordingSource,
        include_cursor: bool,
        plan: H264SinkWriterStreamPlan,
    ) -> Result<Self, RecordingWorkerError> {
        Ok(Self {
            session: WindowsWgcRecordingSession::start(source, include_cursor)
                .map_err(|_| RecordingWorkerError::VideoSource)?,
            clock: VideoFrameClock::new(plan.frame_duration_100ns)
                .map_err(|_| RecordingWorkerError::VideoSource)?,
            plan,
            wall_epoch: None,
            last_frame: None,
        })
    }

    fn sample_from_last(
        &self,
        timestamp_100ns: i64,
    ) -> Result<RecordingVideoPoll, RecordingWorkerError> {
        let frame = self
            .last_frame
            .as_ref()
            .ok_or(RecordingWorkerError::VideoSource)?;
        let duration_100ns = i64::try_from(self.plan.frame_duration_100ns)
            .map_err(|_| RecordingWorkerError::VideoSource)?;
        Ok(RecordingVideoPoll::Sample(OwnedBgraVideoSample {
            width: frame.width,
            height: frame.height,
            stride: frame.stride,
            bgra_bytes: Arc::clone(&frame.bgra_bytes),
            timestamp_100ns,
            duration_100ns,
        }))
    }

    fn repeat_timestamp(&mut self) -> Result<Option<i64>, RecordingWorkerError> {
        let Some(epoch) = self.wall_epoch else {
            return Ok(None);
        };
        let elapsed_100ns = i64::try_from(epoch.elapsed().as_nanos() / 100)
            .map_err(|_| RecordingWorkerError::VideoClock)?;
        self.clock
            .timestamp_for_repeat(elapsed_100ns)
            .map_err(|_| RecordingWorkerError::VideoClock)
    }
}

impl RecordingVideoSource for WindowsWgcRecordingVideoSource {
    fn next_video_sample(&mut self) -> Result<RecordingVideoPoll, RecordingWorkerError> {
        let captured = self
            .session
            .try_next_frame()
            .map_err(recording_frame_error)?;
        let timestamp_100ns = if let Some(captured) = captured {
            let frame = captured.frame;
            if frame.width != self.plan.width || frame.height != self.plan.height {
                return Err(RecordingWorkerError::VideoGeometry);
            }
            self.wall_epoch.get_or_insert_with(Instant::now);
            self.last_frame = Some(CachedBgraFrame {
                width: frame.width,
                height: frame.height,
                stride: frame.stride,
                bgra_bytes: frame.bgra_pixels.into(),
            });
            self.clock
                .timestamp_for_capture(captured.system_relative_time_100ns)
                .map_err(|_| RecordingWorkerError::VideoClock)?
        } else {
            self.repeat_timestamp()?
        };
        match timestamp_100ns {
            Some(timestamp_100ns) => self.sample_from_last(timestamp_100ns),
            None => Ok(RecordingVideoPoll::Pending),
        }
    }
}

const fn recording_frame_error(error: WgcRecordingFrameError) -> RecordingWorkerError {
    match error {
        WgcRecordingFrameError::State => RecordingWorkerError::VideoSource,
        WgcRecordingFrameError::Poll(code) => RecordingWorkerError::VideoPoll(code),
        WgcRecordingFrameError::Timestamp(code) => RecordingWorkerError::VideoTimestamp(code),
        WgcRecordingFrameError::Surface(code) => RecordingWorkerError::VideoSurface(code),
        WgcRecordingFrameError::Interface(code) => RecordingWorkerError::VideoInterface(code),
        WgcRecordingFrameError::Readback => RecordingWorkerError::VideoReadback,
        WgcRecordingFrameError::Composite => RecordingWorkerError::VideoComposite,
    }
}
