//! Windows recording runtime composition.
#![cfg(windows)]

mod active;
mod components;
mod errors;
mod native_error;
mod start;
mod worker_loop;

use std::sync::atomic::Ordering;

use super::{
    FinalizedRecordingArtifact, RecordingRuntime, RecordingRuntimeError,
    RecordingRuntimeStartRequest, RecordingRuntimeStopRequest, StartedRecordingRuntime,
    windows_preflight::windows_recording_runtime_plan,
};
use active::ActiveWindowsRecording;
use start::{spawn_worker, worker_start};

#[derive(Debug, Default)]
pub(crate) struct WindowsRecordingRuntime {
    active: Option<ActiveWindowsRecording>,
}

impl RecordingRuntime for WindowsRecordingRuntime {
    fn start_recording(
        &mut self,
        request: RecordingRuntimeStartRequest<'_>,
    ) -> Result<StartedRecordingRuntime, RecordingRuntimeError> {
        if self.active.is_some() {
            return Err(RecordingRuntimeError::InvalidState);
        }
        let plan = windows_recording_runtime_plan(request.request)?;
        let effective_quality = plan.effective_quality.clone();
        let start = worker_start(&request, plan)?;
        self.active = Some(spawn_worker(request.recording_id, start)?);
        Ok(StartedRecordingRuntime { effective_quality })
    }

    fn stop_recording(
        &mut self,
        request: RecordingRuntimeStopRequest<'_>,
    ) -> Result<FinalizedRecordingArtifact, RecordingRuntimeError> {
        let active = self.take_active(request.recording_id)?;
        active
            .duration_ms
            .store(request.duration_ms, Ordering::Release);
        active.stop_requested.store(true, Ordering::Release);
        active.join()
    }

    fn pause_recording(&mut self, recording_id: &str) -> Result<(), RecordingRuntimeError> {
        let active = self.active_by_id(recording_id)?;
        active.pause_requested.store(true, Ordering::Release);
        Ok(())
    }

    fn resume_recording(&mut self, recording_id: &str) -> Result<(), RecordingRuntimeError> {
        let active = self.active_by_id(recording_id)?;
        active.pause_requested.store(false, Ordering::Release);
        Ok(())
    }

    fn cancel_recording(&mut self, recording_id: &str) -> Result<(), RecordingRuntimeError> {
        let active = self.take_active(recording_id)?;
        active.cancel_requested.store(true, Ordering::Release);
        let _ = active.join();
        Ok(())
    }
}

impl WindowsRecordingRuntime {
    fn active_by_id(
        &self,
        recording_id: &str,
    ) -> Result<&ActiveWindowsRecording, RecordingRuntimeError> {
        self.active
            .as_ref()
            .filter(|active| active.recording_id == recording_id)
            .ok_or(RecordingRuntimeError::InvalidState)
    }

    fn take_active(
        &mut self,
        recording_id: &str,
    ) -> Result<ActiveWindowsRecording, RecordingRuntimeError> {
        let Some(active) = self.active.take() else {
            return Err(RecordingRuntimeError::InvalidState);
        };
        if active.recording_id == recording_id {
            Ok(active)
        } else {
            self.active = Some(active);
            Err(RecordingRuntimeError::InvalidState)
        }
    }
}
