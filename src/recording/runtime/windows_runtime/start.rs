use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64},
        mpsc,
    },
    thread,
};

use crate::{
    protocol::NativeRecordingSource,
    recording::{
        encoder::{Mp4SinkWriterPlan, build_mp4_sink_writer_plan},
        runtime::{
            FinalizedRecordingArtifact, RecordingRuntimeError, RecordingRuntimeFailure,
            RecordingRuntimeStartRequest, plan::RecordingRuntimePlan,
        },
    },
};

use super::{
    active::ActiveWindowsRecording, errors::sink_plan_error,
    worker_loop::run_windows_recording_worker,
};

#[derive(Debug)]
pub(super) struct WindowsRecordingWorkerStart {
    pub(super) source: NativeRecordingSource,
    pub(super) include_cursor: bool,
    pub(super) audio_selection: crate::audio::AudioSourceSelection,
    pub(super) mix: crate::audio::MixPlan,
    pub(super) sink_plan: Mp4SinkWriterPlan,
    pub(super) media_path: PathBuf,
    pub(super) artifact: FinalizedRecordingArtifact,
}

pub(super) fn worker_start(
    request: &RecordingRuntimeStartRequest<'_>,
    plan: RecordingRuntimePlan,
) -> Result<WindowsRecordingWorkerStart, RecordingRuntimeError> {
    let sink_plan = build_mp4_sink_writer_plan(plan.encoder, &plan.mix).map_err(sink_plan_error)?;
    Ok(WindowsRecordingWorkerStart {
        source: request.request.source.clone(),
        include_cursor: request.request.include_cursor,
        audio_selection: plan.audio_selection,
        mix: plan.mix,
        sink_plan,
        media_path: request.media_path.to_path_buf(),
        artifact: FinalizedRecordingArtifact {
            duration_ms: 0,
            width: plan.source.width,
            height: plan.source.height,
            fps: plan.encoder.fps,
        },
    })
}

pub(super) fn spawn_worker(
    recording_id: &str,
    start: WindowsRecordingWorkerStart,
) -> Result<ActiveWindowsRecording, RecordingRuntimeError> {
    let stop_requested = Arc::new(AtomicBool::new(false));
    let cancel_requested = Arc::new(AtomicBool::new(false));
    let pause_requested = Arc::new(AtomicBool::new(false));
    let duration_ms = Arc::new(AtomicU64::new(0));
    let (startup_tx, startup_rx) = mpsc::sync_channel(1);
    let worker_stop = Arc::clone(&stop_requested);
    let worker_cancel = Arc::clone(&cancel_requested);
    let worker_pause = Arc::clone(&pause_requested);
    let worker_duration = Arc::clone(&duration_ms);
    let join = thread::spawn(move || {
        run_windows_recording_worker(
            start,
            startup_tx,
            worker_stop,
            worker_cancel,
            worker_pause,
            worker_duration,
        )
    });
    match startup_rx.recv().map_err(|_| {
        RecordingRuntimeError::RuntimeFailure(RecordingRuntimeFailure::WorkerPanicked)
    })? {
        Ok(()) => Ok(ActiveWindowsRecording {
            recording_id: recording_id.to_owned(),
            stop_requested,
            cancel_requested,
            pause_requested,
            duration_ms,
            join: Some(join),
        }),
        Err(error) => {
            let _ = join.join();
            Err(error)
        }
    }
}
