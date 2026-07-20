//! Shared, coalescing recording state for the tray projection.

use std::sync::{Arc, Mutex, mpsc::SyncSender};

use crate::tray::{TrayCommandSource, TrayRecordingStatus};

use super::{AgentShutdownSignal, TrayWorker};

#[derive(Clone, Debug)]
pub(crate) struct TrayRecordingState(Arc<Mutex<TrayRecordingStatus>>);

impl Default for TrayRecordingState {
    fn default() -> Self {
        Self(Arc::new(Mutex::new(TrayRecordingStatus::Idle)))
    }
}

impl TrayRecordingState {
    pub(crate) fn status(&self) -> TrayRecordingStatus {
        self.0
            .lock()
            .map_or(TrayRecordingStatus::Idle, |status| *status)
    }

    pub(crate) fn set(&self, status: TrayRecordingStatus) {
        if let Ok(mut current) = self.0.lock() {
            *current = status;
        }
    }
}

impl TrayWorker {
    pub(super) fn sync_recording_status(
        &mut self,
        shutdown_sender: &SyncSender<AgentShutdownSignal>,
        source: &mut impl TrayCommandSource,
    ) {
        let status = self.recording_state.status();
        if self.icon_state.recording != status {
            self.apply_recording_status(status, shutdown_sender, source);
        }
    }

    pub(super) fn apply_recording_status(
        &mut self,
        status: TrayRecordingStatus,
        shutdown_sender: &SyncSender<AgentShutdownSignal>,
        source: &mut impl TrayCommandSource,
    ) {
        self.icon_state.recording = status;
        self.render_current_menu(source);
        if matches!(status, TrayRecordingStatus::Idle) && self.exit_after_recording {
            self.exit_after_recording = false;
            let _ = shutdown_sender.try_send(AgentShutdownSignal::Stop);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::TrayRecordingState;
    use crate::tray::TrayRecordingStatus;

    #[test]
    fn recording_state_coalesces_to_the_latest_projection() {
        let state = TrayRecordingState::default();
        let observer = state.clone();

        state.set(TrayRecordingStatus::Recording);
        state.set(TrayRecordingStatus::Finalizing);
        state.set(TrayRecordingStatus::Idle);

        assert_eq!(observer.status(), TrayRecordingStatus::Idle);
    }
}
