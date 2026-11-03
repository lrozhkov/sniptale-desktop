//! Bounded hook event polling on the shared recording monotonic clock.

use crate::{
    app::resident::{AgentComponent, WorkerCommand, WorkerEvent},
    telemetry::RecordingTelemetrySource,
};
use std::sync::mpsc::{Receiver, RecvTimeoutError, SyncSender};

pub(in crate::app::resident) fn run_telemetry_worker(
    component: AgentComponent,
    event_sender: &SyncSender<WorkerEvent>,
    command_receiver: &Receiver<WorkerCommand>,
    mut source: Box<dyn RecordingTelemetrySource>,
) {
    let _ = event_sender.send(WorkerEvent::Started(component));
    loop {
        match command_receiver.recv_timeout(std::time::Duration::from_millis(16)) {
            Ok(WorkerCommand::Stop) | Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => {
                source.poll_recording_events(crate::recording::recording_monotonic_ns());
            }
            Ok(WorkerCommand::FenceMode { result_sender }) => {
                let _ = result_sender.send(false);
            }
            Ok(WorkerCommand::StartRecordingTelemetry {
                policy,
                trajectory_context,
                recording_id,
                started_at_monotonic_ns,
            }) => {
                source.configure_trajectory(trajectory_context);
                source.configure_recording(policy);
                source.start_recording(&recording_id, started_at_monotonic_ns);
            }
            Ok(WorkerCommand::PauseRecordingTelemetry {
                recording_id,
                paused_at_monotonic_ns,
            }) => source.pause_recording(&recording_id, paused_at_monotonic_ns),
            Ok(WorkerCommand::ResumeRecordingTelemetry {
                recording_id,
                resumed_at_monotonic_ns,
            }) => source.resume_recording(&recording_id, resumed_at_monotonic_ns),
            Ok(WorkerCommand::FinishRecordingTelemetry {
                recording_id,
                stopped_at_monotonic_ns,
            }) => source.finish_recording(&recording_id, stopped_at_monotonic_ns),
            Ok(WorkerCommand::CancelRecordingTelemetry {
                recording_id,
                cancelled_at_monotonic_ns,
            }) => source.cancel_recording(&recording_id, cancelled_at_monotonic_ns),
            Ok(WorkerCommand::DiscardRecordingTelemetry { recording_id }) => {
                source.discard_recording(&recording_id);
            }
            Ok(WorkerCommand::SnapshotRecordingTelemetry {
                recording_id,
                duration_ms,
                result_sender,
            }) => {
                let _ =
                    result_sender.send(source.snapshot_for_recording(&recording_id, duration_ms));
            }
            Ok(
                WorkerCommand::ApplyHotkeyPlan { .. }
                | WorkerCommand::ManualHotkeyAction { .. }
                | WorkerCommand::DesktopCapture { .. }
                | WorkerCommand::DesktopSettings { .. }
                | WorkerCommand::RestoreHotkeys { .. }
                | WorkerCommand::ApplyTrayRegistry { .. }
                | WorkerCommand::ShowTrayNotice { .. }
                | WorkerCommand::UpdateTrayRecordingStatus { .. }
                | WorkerCommand::RefreshTrayConnectionStatus
                | WorkerCommand::CaptureScreenshot { .. }
                | WorkerCommand::StartRecording { .. }
                | WorkerCommand::ControlRecording { .. }
                | WorkerCommand::ControlObservedRecording { .. }
                | WorkerCommand::ControlActiveRecording { .. }
                | WorkerCommand::PrepareManualAsset { .. }
                | WorkerCommand::PublishManualAsset { .. }
                | WorkerCommand::RecoverPendingAssets
                | WorkerCommand::ChooseMediaDestination { .. },
            ) => {}
            #[cfg(test)]
            Ok(WorkerCommand::TrayMenuCommandSelected { .. }) => {}
        }
    }
}
