use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

use crate::recording::runtime::{
    FinalizedRecordingArtifact, RecordingRuntimeError,
    worker::{RecordingMediaPump, RecordingPumpState},
};

use super::{
    components::create_worker_components,
    errors::{sink_writer_finalize_error, worker_error},
    start::WindowsRecordingWorkerStart,
};

const RECORDING_IDLE_POLL_INTERVAL: Duration = Duration::from_millis(2);

pub(super) fn run_windows_recording_worker(
    start: WindowsRecordingWorkerStart,
    startup_tx: mpsc::SyncSender<Result<(), RecordingRuntimeError>>,
    stop_requested: Arc<AtomicBool>,
    cancel_requested: Arc<AtomicBool>,
    pause_requested: Arc<AtomicBool>,
    duration_ms: Arc<AtomicU64>,
) -> Result<FinalizedRecordingArtifact, RecordingRuntimeError> {
    let mut artifact = start.artifact;
    let components = create_worker_components(start);
    match components {
        Ok((video, audio, sink)) => {
            let _ = startup_tx.send(Ok(()));
            let mut pump = RecordingMediaPump::new(video, audio, sink);
            loop {
                if cancel_requested.load(Ordering::Acquire) {
                    return Ok(artifact);
                }
                if stop_requested.load(Ordering::Acquire) {
                    // Do not poll a live WASAPI endpoint after stop. The regular pump drains every
                    // ready packet between video frames; a final synchronous drain can keep
                    // producing packets and block Media Foundation interleaving before Finalize.
                    break;
                }
                let state = if pause_requested.load(Ordering::Acquire) {
                    RecordingPumpState::Paused
                } else {
                    RecordingPumpState::Active
                };
                let progress = pump.pump_once_with_state(state).map_err(worker_error)?;
                if progress.video_samples == 0 && progress.audio_samples == 0 {
                    thread::sleep(RECORDING_IDLE_POLL_INTERVAL);
                } else {
                    thread::yield_now();
                }
            }
            tracing::info!(
                target: "sniptale_native_agent::recording",
                phase = "recording-stop-observed",
                "recording worker stopped media pumping"
            );
            let (video, audio, mut sink) = pump.into_parts();
            drop(video);
            drop(audio);
            tracing::info!(
                target: "sniptale_native_agent::recording",
                phase = "recording-sources-stopped",
                "recording capture sources stopped"
            );
            tracing::info!(
                target: "sniptale_native_agent::recording",
                phase = "recording-finalize-start",
                "recording sink finalization started"
            );
            sink.finalize().map_err(sink_writer_finalize_error)?;
            tracing::info!(
                target: "sniptale_native_agent::recording",
                phase = "recording-finalize-finish",
                "recording sink finalization completed"
            );
            artifact.duration_ms = duration_ms.load(Ordering::Acquire);
            Ok(artifact)
        }
        Err(error) => {
            let _ = startup_tx.send(Err(error));
            Err(error)
        }
    }
}
