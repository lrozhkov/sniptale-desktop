use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64},
    },
    thread::JoinHandle,
};

use crate::recording::runtime::{
    FinalizedRecordingArtifact, RecordingRuntimeError, RecordingRuntimeFailure,
};

#[derive(Debug)]
pub(super) struct ActiveWindowsRecording {
    pub(super) recording_id: String,
    pub(super) stop_requested: Arc<AtomicBool>,
    pub(super) cancel_requested: Arc<AtomicBool>,
    pub(super) pause_requested: Arc<AtomicBool>,
    pub(super) duration_ms: Arc<AtomicU64>,
    pub(super) join: Option<JoinHandle<Result<FinalizedRecordingArtifact, RecordingRuntimeError>>>,
}

impl ActiveWindowsRecording {
    pub(super) fn join(mut self) -> Result<FinalizedRecordingArtifact, RecordingRuntimeError> {
        let join = self
            .join
            .take()
            .ok_or(RecordingRuntimeError::InvalidState)?;
        join.join().map_err(|_| {
            RecordingRuntimeError::RuntimeFailure(RecordingRuntimeFailure::WorkerPanicked)
        })?
    }
}
