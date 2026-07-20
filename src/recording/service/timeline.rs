//! Recording media-duration timeline and pause compaction.

use super::super::session::RecordingControl;

#[derive(Debug, Default, Eq, PartialEq)]
pub(super) struct RecordingTimeline {
    started_epoch: Option<u64>,
    open_pause_started_epoch: Option<u64>,
    paused_total: u64,
}

impl RecordingTimeline {
    pub(super) fn start(&mut self, started_at_epoch_ms: u64) {
        self.started_epoch = Some(started_at_epoch_ms);
        self.open_pause_started_epoch = None;
        self.paused_total = 0;
    }

    pub(super) fn record_control(&mut self, control: RecordingControl, observed_epoch_ms: u64) {
        match control {
            RecordingControl::Pause => {
                self.open_pause_started_epoch = Some(observed_epoch_ms);
            }
            RecordingControl::Resume | RecordingControl::Stop => {
                self.close_pause_interval(observed_epoch_ms);
            }
            RecordingControl::Cancel => {}
        }
    }

    pub(super) fn active_duration_ms(&self, observed_epoch_ms: u64) -> u64 {
        let elapsed =
            observed_epoch_ms.saturating_sub(self.started_epoch.unwrap_or(observed_epoch_ms));
        elapsed.saturating_sub(self.total_paused_ms_at(observed_epoch_ms))
    }

    pub(super) fn reset(&mut self) {
        self.started_epoch = None;
        self.open_pause_started_epoch = None;
        self.paused_total = 0;
    }

    fn close_pause_interval(&mut self, observed_epoch_ms: u64) {
        let Some(paused_at_epoch_ms) = self.open_pause_started_epoch.take() else {
            return;
        };
        self.paused_total = self
            .paused_total
            .saturating_add(observed_epoch_ms.saturating_sub(paused_at_epoch_ms));
    }

    fn total_paused_ms_at(&self, observed_epoch_ms: u64) -> u64 {
        let open_pause_ms = self
            .open_pause_started_epoch
            .map_or(0, |paused_at| observed_epoch_ms.saturating_sub(paused_at));
        self.paused_total.saturating_add(open_pause_ms)
    }
}
