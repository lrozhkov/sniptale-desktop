//! Resident audio worker lifecycle supervisor.

use std::sync::mpsc::{Receiver, SyncSender};

use super::{AgentComponent, WorkerCommand, WorkerEvent};

pub(super) fn run_audio_worker(
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

#[cfg(test)]
mod tests {
    use std::{sync::mpsc, thread, time::Duration};

    use super::*;

    #[test]
    fn audio_worker_starts_and_stops_on_command() -> Result<(), String> {
        let (command_sender, command_receiver) = mpsc::sync_channel(1);
        let (event_sender, event_receiver) = mpsc::sync_channel(1);
        let handle = thread::spawn(move || {
            run_audio_worker(
                AgentComponent::AudioWorker,
                &event_sender,
                &command_receiver,
            );
        });

        assert_eq!(
            event_receiver.recv_timeout(Duration::from_millis(500)),
            Ok(WorkerEvent::Started(AgentComponent::AudioWorker))
        );
        command_sender
            .send(WorkerCommand::Stop)
            .map_err(|error| error.to_string())?;
        handle.join().map_err(|_| "join worker".to_owned())?;
        Ok(())
    }
}
