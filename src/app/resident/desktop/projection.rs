//! Coalesced presentation facts. This does not admit or execute media operations.

use crate::tray::desktop::model::{
    CaptureCapabilities, CaptureCommand, CaptureKind, CaptureOutcome, CapturePhase, CaptureQuality,
    CaptureSnapshot, DesktopError,
};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug)]
pub(crate) struct CaptureProjection(Arc<Mutex<Projection>>);

#[derive(Debug)]
struct Projection {
    operation: Option<String>,
    job: Option<String>,
    recording: Option<String>,
    revision: u64,
    operation_epoch: u64,
    kind: CaptureKind,
    phase: CapturePhase,
    elapsed_ms: u64,
    outcome: Option<CaptureOutcome>,
}

impl Default for CaptureProjection {
    fn default() -> Self {
        Self(Arc::new(Mutex::new(Projection {
            operation: None,
            job: None,
            recording: None,
            revision: 1,
            operation_epoch: 0,
            kind: CaptureKind::Screenshot,
            phase: CapturePhase::Idle,
            elapsed_ms: 0,
            outcome: None,
        })))
    }
}

impl CaptureProjection {
    pub(crate) fn begin(
        &self,
        operation: &str,
        job: Option<&str>,
        kind: CaptureKind,
        phase: CapturePhase,
        recording: Option<&str>,
    ) {
        if let Ok(mut state) = self.0.lock() {
            if state.operation.as_deref() == Some(operation) {
                return;
            }
            // Keep recording controls attached while a parallel screenshot completes.
            // A new recording may replace one whose media is already finalizing.
            if state.kind == CaptureKind::Video
                && !state.terminal()
                && (kind == CaptureKind::Screenshot || state.phase != CapturePhase::Saving)
            {
                return;
            }
            state.operation_epoch = state
                .operation_epoch
                .saturating_add(1)
                .min(9_007_199_254_740_991);
            state.operation = Some(operation.to_owned());
            state.job = job.map(str::to_owned);
            state.recording = recording.map(str::to_owned);
            state.kind = kind;
            state.elapsed_ms = 0;
            state.outcome = None;
            state.phase = phase;
            state.bump();
        }
    }

    pub(crate) fn epoch_matches(&self, expected: u64) -> bool {
        self.0
            .lock()
            .is_ok_and(|state| state.operation_epoch == expected)
    }

    pub(crate) fn phase(&self) -> Result<CapturePhase, DesktopError> {
        self.0
            .lock()
            .map(|state| state.phase)
            .map_err(|_| DesktopError::Unavailable)
    }

    pub(crate) fn recording_identity(&self, expected_epoch: u64) -> Result<String, DesktopError> {
        let state = self.0.lock().map_err(|_| DesktopError::Unavailable)?;
        if state.operation_epoch != expected_epoch
            || !matches!(state.phase, CapturePhase::Recording | CapturePhase::Paused)
        {
            return Err(DesktopError::Rejected);
        }
        state.recording.clone().ok_or(DesktopError::Unavailable)
    }

    pub(crate) fn advance(&self, operation: &str, phase: CapturePhase) {
        if let Ok(mut state) = self.0.lock()
            && state.operation.as_deref() == Some(operation)
            && !state.terminal()
            && state.phase != phase
        {
            state.phase = phase;
            state.bump();
        }
    }

    pub(crate) fn reject_pending(&self, operation: &str) {
        if let Ok(mut state) = self.0.lock()
            && state.operation.as_deref() == Some(operation)
            && matches!(
                state.phase,
                CapturePhase::Starting | CapturePhase::Selecting
            )
        {
            state.phase = CapturePhase::Failed;
            state.outcome = Some(CaptureOutcome::CaptureFailed);
            state.bump();
        }
    }

    pub(crate) fn fail(&self, operation: &str) {
        self.fail_as(operation, CaptureOutcome::CaptureFailed);
    }
    pub(crate) fn fail_save(&self, operation: &str) {
        self.fail_as(operation, CaptureOutcome::SaveFailed);
    }

    fn fail_as(&self, operation: &str, outcome: CaptureOutcome) {
        if let Ok(mut state) = self.0.lock()
            && state.operation.as_deref() == Some(operation)
            && !state.terminal()
        {
            state.phase = CapturePhase::Failed;
            state.outcome = Some(outcome);
            state.bump();
        }
    }

    pub(crate) fn recording(&self, recording: &str, phase: CapturePhase, elapsed_ms: u64) {
        if let Ok(mut state) = self.0.lock()
            && state.recording.as_deref() == Some(recording)
            && !state.terminal()
            && state.phase != CapturePhase::Saving
            && elapsed_ms >= state.elapsed_ms
        {
            if state.phase == phase && state.elapsed_ms == elapsed_ms {
                return;
            }
            state.phase = phase;
            // Presentation numbers must remain exact in JavaScript. Media limits remain owner-local.
            state.elapsed_ms = elapsed_ms.min(9_007_199_254_740_991);
            state.bump();
        }
    }

    pub(crate) fn cancel(&self, operation: &str) {
        if let Ok(mut state) = self.0.lock()
            && state.operation.as_deref() == Some(operation)
            && !state.terminal()
        {
            state.phase = CapturePhase::Complete;
            state.outcome = Some(CaptureOutcome::Cancelled);
            state.bump();
        }
    }

    pub(crate) fn recording_finalized(&self, recording: &str) {
        if let Ok(mut state) = self.0.lock()
            && state.recording.as_deref() == Some(recording)
            && !state.terminal()
            && state.phase != CapturePhase::Saving
        {
            state.phase = CapturePhase::Saving;
            state.bump();
        }
    }

    pub(crate) fn recording_failed(&self, recording: &str) {
        let operation = self.0.lock().ok().and_then(|state| {
            (state.recording.as_deref() == Some(recording))
                .then(|| state.operation.clone())
                .flatten()
        });
        if let Some(operation) = operation {
            self.fail_save(&operation);
        }
    }

    pub(crate) fn recording_discarded(&self, recording: &str) {
        if let Ok(mut state) = self.0.lock()
            && state.recording.as_deref() == Some(recording)
            && !state.terminal()
        {
            state.phase = CapturePhase::Complete;
            state.outcome = Some(CaptureOutcome::Discarded);
            state.bump();
        }
    }

    pub(crate) fn recording_completed(&self, recording: &str) {
        if let Ok(mut state) = self.0.lock()
            && state.recording.as_deref() == Some(recording)
            && state.job.is_none()
            && !state.terminal()
        {
            // Finalized browser-transfer media is not a durable publication receipt.
            state.phase = CapturePhase::Complete;
            state.outcome = None;
            state.bump();
        }
    }

    pub(crate) fn publication(&self, job: &str, saved: bool) {
        if let Ok(mut state) = self.0.lock()
            && state.job.as_deref() == Some(job)
            && state.phase == CapturePhase::Saving
        {
            state.phase = if saved {
                CapturePhase::Complete
            } else {
                CapturePhase::Failed
            };
            state.outcome = Some(if saved {
                CaptureOutcome::Saved
            } else {
                CaptureOutcome::SaveFailed
            });
            state.bump();
        }
    }

    pub(crate) fn snapshot(
        &self,
        capabilities: CaptureCapabilities,
        default_quality: CaptureQuality,
    ) -> Result<CaptureSnapshot, DesktopError> {
        let state = self.0.lock().map_err(|_| DesktopError::Unavailable)?;
        let commands = match state.phase {
            CapturePhase::Idle | CapturePhase::Complete | CapturePhase::Failed => {
                vec![CaptureCommand::Start]
            }
            CapturePhase::Recording => vec![
                CaptureCommand::Pause,
                CaptureCommand::Stop,
                CaptureCommand::Discard,
            ],
            CapturePhase::Paused => vec![
                CaptureCommand::Resume,
                CaptureCommand::Stop,
                CaptureCommand::Discard,
            ],
            CapturePhase::Selecting | CapturePhase::Starting | CapturePhase::Saving => Vec::new(),
        };
        Ok(CaptureSnapshot {
            revision: state.revision,
            operation_epoch: state.operation_epoch,
            default_quality,
            setup_required: false,
            phase: state.phase,
            kind: state.kind,
            elapsed_ms: state.elapsed_ms,
            commands,
            capabilities,
            outcome: state.outcome,
        })
    }
}

impl Projection {
    fn terminal(&self) -> bool {
        matches!(self.phase, CapturePhase::Complete | CapturePhase::Failed)
    }
    fn bump(&mut self) {
        self.revision = self.revision.saturating_add(1).min(9_007_199_254_740_991);
    }
}

#[cfg(test)]
pub(crate) mod tests;
