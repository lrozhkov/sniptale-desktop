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
    // Cold codec activation can take several seconds. Complete it before either live source
    // starts, so buffered capture timestamps cannot include encoder initialization time.
    let sink = WindowsMp4SinkWriter::create(&start.media_path, &start.sink_plan)
        .map_err(sink_writer_create_error)?;
    let audio_pipeline = windows_audio_pipeline(&start.audio_selection, start.mix)?;
    let audio = RecordingPcmAudioSource::new(audio_pipeline);
    let video = WindowsWgcRecordingVideoSource::start(
        &start.source,
        start.include_cursor,
        start.sink_plan.video.clone(),
    )
    .map_err(worker_error)?;
    Ok((video, audio, sink))
}
