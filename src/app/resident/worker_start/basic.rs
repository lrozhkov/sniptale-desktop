//! Fallback worker loop for components without a specialized runtime.

use std::sync::mpsc::{Receiver, SyncSender};

use super::super::{AgentComponent, WorkerCommand, WorkerEvent};

pub(super) fn run_basic_worker(
    component: AgentComponent,
    event_sender: &SyncSender<WorkerEvent>,
    command_receiver: &Receiver<WorkerCommand>,
) {
    let _ = event_sender.send(WorkerEvent::Started(component));
    loop {
        match command_receiver.recv() {
            Ok(WorkerCommand::Stop) | Err(_) => break,
            Ok(WorkerCommand::FenceMode { result_sender }) => {
                let _ = result_sender.send(false);
            }
            Ok(
                WorkerCommand::ApplyHotkeyPlan { .. }
                | WorkerCommand::ManualHotkeyAction { .. }
                | WorkerCommand::RestoreHotkeys { .. }
                | WorkerCommand::ApplyTrayRegistry { .. }
                | WorkerCommand::ShowTrayNotice { .. }
                | WorkerCommand::UpdateTrayRecordingStatus { .. }
                | WorkerCommand::RefreshTrayConnectionStatus
                | WorkerCommand::CaptureScreenshot { .. }
                | WorkerCommand::StartRecording { .. }
                | WorkerCommand::ControlRecording { .. }
                | WorkerCommand::ControlActiveRecording { .. }
                | WorkerCommand::StartRecordingTelemetry { .. }
                | WorkerCommand::PauseRecordingTelemetry { .. }
                | WorkerCommand::ResumeRecordingTelemetry { .. }
                | WorkerCommand::FinishRecordingTelemetry { .. }
                | WorkerCommand::CancelRecordingTelemetry { .. }
                | WorkerCommand::DiscardRecordingTelemetry { .. }
                | WorkerCommand::SnapshotRecordingTelemetry { .. }
                | WorkerCommand::PrepareManualAsset { .. }
                | WorkerCommand::PublishManualAsset { .. },
            ) => {}
            #[cfg(test)]
            Ok(WorkerCommand::TrayMenuCommandSelected { .. }) => {}
        }
    }
}
