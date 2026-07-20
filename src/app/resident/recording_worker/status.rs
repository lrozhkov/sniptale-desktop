//! Recording-to-tray status projection.

use std::sync::mpsc::SyncSender;

use crate::app::resident::{WorkerCommand, tray::TrayRecordingState};

pub(super) fn set_tray_recording_status(
    state: &TrayRecordingState,
    tray_command_sender: Option<&SyncSender<WorkerCommand>>,
    status: crate::tray::TrayRecordingStatus,
) {
    // Recording lifecycle owns the authoritative latest state. The command is only a best-effort
    // wake-up for prompt rendering; the tray polls the shared projection and therefore converges
    // even when its bounded command queue is full. Media control must never block behind UI work.
    state.set(status);
    if let Some(sender) = tray_command_sender {
        let _ = sender.try_send(WorkerCommand::UpdateTrayRecordingStatus { status });
    }
}
