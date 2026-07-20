//! Windows Media Foundation MP4 Sink Writer adapter.
#![cfg(windows)]
#![allow(unsafe_code)]

mod media_types;
mod sample_buffers;

use std::{os::windows::ffi::OsStrExt, path::Path};

use windows::{
    Win32::{
        Foundation::E_POINTER,
        Media::MediaFoundation::{
            IMFByteStream, IMFSinkWriter, MF_SINK_WRITER_DISABLE_THROTTLING, MFCreateAttributes,
            MFCreateSinkWriterFromURL,
        },
    },
    core::PCWSTR,
};

use crate::recording::host_runtime::MediaFoundationRuntime;

use super::{
    Mp4SinkWriterPlan,
    samples::{BgraVideoSample, PcmAudioSample, SinkWriterSampleError},
};
use crate::recording::runtime::worker::{RecordingMediaSink, RecordingWorkerError};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Mp4SinkWriterError {
    PathContainsNul,
    InvalidCodecValue,
    InvalidSample(SinkWriterSampleError),
    Windows(i32),
}

impl From<SinkWriterSampleError> for Mp4SinkWriterError {
    fn from(error: SinkWriterSampleError) -> Self {
        Self::InvalidSample(error)
    }
}

#[derive(Debug)]
pub(crate) struct WindowsMp4SinkWriter {
    _runtime: MediaFoundationRuntime,
    writer: IMFSinkWriter,
    plan: Mp4SinkWriterPlan,
    pub(crate) video_stream_index: u32,
    pub(crate) audio_stream_index: u32,
    finalized: bool,
}

impl WindowsMp4SinkWriter {
    pub(crate) fn create(
        path: &Path,
        plan: &Mp4SinkWriterPlan,
    ) -> Result<Self, Mp4SinkWriterError> {
        let output_path = wide_path(path)?;
        let runtime = MediaFoundationRuntime::startup().map_err(to_error)?;
        let writer = create_sink_writer(&output_path)?;
        let video_stream_index = media_types::add_h264_stream(&writer, &plan.video)?;
        let audio_stream_index = media_types::add_aac_stream(&writer, &plan.audio)?;
        media_types::set_rgb32_input_type(&writer, video_stream_index, &plan.video)?;
        media_types::set_pcm_input_type(&writer, audio_stream_index, &plan.audio)?;
        // SAFETY: the sink writer has both target streams and input media types configured. The
        // writer COM object remains owned by this RAII adapter until explicit finalize or drop.
        unsafe { writer.BeginWriting() }.map_err(to_error)?;
        Ok(Self {
            _runtime: runtime,
            writer,
            plan: plan.clone(),
            video_stream_index,
            audio_stream_index,
            finalized: false,
        })
    }

    pub(crate) fn write_video_frame(
        &mut self,
        sample: BgraVideoSample<'_>,
    ) -> Result<(), Mp4SinkWriterError> {
        let bytes = sample_buffers::bgra_video_bytes(&self.plan.video, sample)?;
        let media_sample = sample_buffers::media_sample_from_bytes(
            bytes.as_ref(),
            sample.timestamp_100ns,
            sample.duration_100ns,
        )?;
        // SAFETY: stream_index came from this writer's H.264 stream. The sample buffer was created
        // by this adapter and validated against the configured video stream plan.
        unsafe {
            self.writer
                .WriteSample(self.video_stream_index, &media_sample)
        }
        .map_err(to_error)
    }

    pub(crate) fn write_pcm_audio_sample(
        &mut self,
        sample: PcmAudioSample<'_>,
    ) -> Result<(), Mp4SinkWriterError> {
        sample_buffers::validate_pcm_audio_bytes(&self.plan.audio, sample)?;
        let media_sample = sample_buffers::media_sample_from_bytes(
            sample.bytes,
            sample.timestamp_100ns,
            sample.duration_100ns,
        )?;
        // SAFETY: stream_index came from this writer's AAC stream. The sample buffer was created
        // by this adapter and validated against the configured audio stream plan.
        unsafe {
            self.writer
                .WriteSample(self.audio_stream_index, &media_sample)
        }
        .map_err(to_error)
    }

    pub(crate) fn finalize(&mut self) -> Result<(), Mp4SinkWriterError> {
        if self.finalized {
            return Ok(());
        }
        // SAFETY: finalizes the live sink writer exactly once from the owner that created it.
        unsafe { self.writer.Finalize() }.map_err(to_error)?;
        self.finalized = true;
        Ok(())
    }
}

impl RecordingMediaSink for WindowsMp4SinkWriter {
    fn write_video_sample(
        &mut self,
        sample: BgraVideoSample<'_>,
    ) -> Result<(), RecordingWorkerError> {
        self.write_video_frame(sample).map_err(worker_error)
    }

    fn write_audio_sample(
        &mut self,
        sample: PcmAudioSample<'_>,
    ) -> Result<(), RecordingWorkerError> {
        self.write_pcm_audio_sample(sample).map_err(worker_error)
    }
}

fn create_sink_writer(output_path: &[u16]) -> Result<IMFSinkWriter, Mp4SinkWriterError> {
    let mut attributes = None;
    // SAFETY: attributes is a valid out-parameter. The returned COM owner remains live through
    // sink-writer creation and is released by windows-rs after the call.
    unsafe { MFCreateAttributes(&mut attributes, 1) }.map_err(to_error)?;
    let attributes = attributes.ok_or(Mp4SinkWriterError::Windows(E_POINTER.0))?;
    // SAFETY: the attribute store is exclusively initialized here. Recording owns pacing and
    // bounds audio against the published video timeline, so WriteSample must remain stop-aware
    // instead of blocking the worker with Media Foundation's default wall-clock throttling.
    unsafe { attributes.SetUINT32(&MF_SINK_WRITER_DISABLE_THROTTLING, 1) }.map_err(to_error)?;
    // SAFETY: output_path is NUL-terminated and lives through the call. The byte stream is null so
    // Media Foundation creates the MP4 sink from the file extension; attributes lives through the
    // synchronous call and configures only the real-time pacing/buffering policy.
    unsafe {
        MFCreateSinkWriterFromURL(
            PCWSTR(output_path.as_ptr()),
            None::<&IMFByteStream>,
            Some(&attributes),
        )
    }
    .map_err(to_error)
}

fn wide_path(path: &Path) -> Result<Vec<u16>, Mp4SinkWriterError> {
    let mut wide = path.as_os_str().encode_wide().collect::<Vec<_>>();
    if wide.contains(&0) {
        return Err(Mp4SinkWriterError::PathContainsNul);
    }
    wide.push(0);
    Ok(wide)
}

fn to_error(error: windows::core::Error) -> Mp4SinkWriterError {
    Mp4SinkWriterError::Windows(error.code().0)
}

fn worker_error(error: Mp4SinkWriterError) -> RecordingWorkerError {
    match error {
        Mp4SinkWriterError::InvalidSample(error) => RecordingWorkerError::Sink(error),
        Mp4SinkWriterError::PathContainsNul | Mp4SinkWriterError::InvalidCodecValue => {
            RecordingWorkerError::SinkFailed
        }
        Mp4SinkWriterError::Windows(code) => RecordingWorkerError::SinkNative(code),
    }
}
