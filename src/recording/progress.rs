//! Recording progress cadence and coalescing policy.

use crate::protocol::RecordingProgressStatus;

pub(crate) const DEFAULT_PROGRESS_MIN_INTERVAL_MS: u64 = 500;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RecordingProgressSnapshot {
    pub(crate) duration_ms: u64,
    pub(crate) status: RecordingProgressStatus,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct EmittedProgress {
    observed_at_epoch_ms: u64,
    snapshot: RecordingProgressSnapshot,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RecordingProgressEmitter {
    min_interval_ms: u64,
    last_emitted: Option<EmittedProgress>,
    pending: Option<RecordingProgressSnapshot>,
}

impl Default for RecordingProgressEmitter {
    fn default() -> Self {
        Self::new(DEFAULT_PROGRESS_MIN_INTERVAL_MS)
    }
}

impl RecordingProgressEmitter {
    pub(crate) const fn new(min_interval_ms: u64) -> Self {
        Self {
            min_interval_ms,
            last_emitted: None,
            pending: None,
        }
    }

    pub(crate) fn observe(
        &mut self,
        observed_at_epoch_ms: u64,
        snapshot: RecordingProgressSnapshot,
    ) -> Option<RecordingProgressSnapshot> {
        if self.should_emit(observed_at_epoch_ms, &snapshot) {
            Some(self.force_emit(observed_at_epoch_ms, snapshot))
        } else {
            self.pending = Some(snapshot);
            None
        }
    }

    pub(crate) fn force_emit(
        &mut self,
        observed_at_epoch_ms: u64,
        snapshot: RecordingProgressSnapshot,
    ) -> RecordingProgressSnapshot {
        self.last_emitted = Some(EmittedProgress {
            observed_at_epoch_ms,
            snapshot: snapshot.clone(),
        });
        self.pending = None;
        snapshot
    }

    pub(crate) fn reset(&mut self) {
        self.last_emitted = None;
        self.pending = None;
    }

    fn should_emit(&self, observed_at_epoch_ms: u64, snapshot: &RecordingProgressSnapshot) -> bool {
        let Some(last) = &self.last_emitted else {
            return true;
        };
        if snapshot.status != last.snapshot.status {
            return true;
        }
        observed_at_epoch_ms.saturating_sub(last.observed_at_epoch_ms) >= self.min_interval_ms
    }
}
