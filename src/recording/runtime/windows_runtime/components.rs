use crate::recording::{
    encoder::windows_sink_writer::WindowsMp4SinkWriter,
    runtime::{
        RecordingRuntimeError,
        audio_source::RecordingPcmAudioSource,
        windows_audio::{WindowsAudioPipeline, windows_audio_pipeline},
        windows_video::WindowsWgcRecordingVideoSource,
    },
};

use super::{
    errors::{sink_writer_create_error, worker_error},
    start::WindowsRecordingWorkerStart,
};

type WindowsRecordingAudioSource = RecordingPcmAudioSource<WindowsAudioPipeline>;

pub(super) fn create_worker_components(
    start: WindowsRecordingWorkerStart,
) -> Result<
    (
        WindowsWgcRecordingVideoSource,
        WindowsRecordingAudioSource,
        WindowsMp4SinkWriter,
    ),
    RecordingRuntimeError,
> {
    let video = WindowsWgcRecordingVideoSource::start(
        &start.source,
        start.include_cursor,
        start.sink_plan.video.clone(),
    )
    .map_err(worker_error)?;
    let audio_pipeline = windows_audio_pipeline(&start.audio_selection, start.mix)?;
    let audio = RecordingPcmAudioSource::new(audio_pipeline);
    let sink = WindowsMp4SinkWriter::create(&start.media_path, &start.sink_plan)
        .map_err(sink_writer_create_error)?;
    Ok((video, audio, sink))
}
