//! Resident telemetry worker command broker.

use std::sync::mpsc::{Receiver, RecvTimeoutError, SyncSender, TrySendError};

use crate::{
    app::responses::NativeHostCommandFailureEvent,
    protocol::{OperationFailedOperation, OperationalErrorKind},
    telemetry::{
        RecordingTelemetryPolicy, RecordingTelemetrySource, SanitizedRecordingTelemetrySnapshot,
    },
};

use super::{AgentComponent, AgentEventHub, RUNTIME_APPLY_ACK_TIMEOUT, WorkerCommand, WorkerEvent};

#[derive(Clone, Debug)]
pub(super) struct TelemetryWorkerCommandSink {
    sender: Option<SyncSender<WorkerCommand>>,
    agent_events: AgentEventHub,
    controller_lease_id: Option<String>,
    policy: RecordingTelemetryPolicy,
}

impl TelemetryWorkerCommandSink {
    pub(super) fn new(
        sender: Option<SyncSender<WorkerCommand>>,
        agent_events: AgentEventHub,
    ) -> Self {
        Self {
            sender,
            agent_events,
            controller_lease_id: None,
            policy: RecordingTelemetryPolicy::default(),
        }
    }

    fn enqueue(&self, command: WorkerCommand, phase: &'static str, operation_id: Option<&str>) {
        let Some(sender) = self.sender.as_ref() else {
            self.publish_failure(
                phase,
                operation_id,
                OperationalErrorKind::UnsupportedCapability,
            );
            return;
        };
        match sender.try_send(command) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => {
                self.publish_failure(phase, operation_id, OperationalErrorKind::QueueBackpressure);
            }
            Err(TrySendError::Disconnected(_)) => {
                self.publish_failure(
                    phase,
                    operation_id,
                    OperationalErrorKind::CommandDispatchFailed,
                );
            }
        }
    }

    fn enqueue_required(
        &self,
        command: WorkerCommand,
        phase: &'static str,
        operation_id: Option<&str>,
    ) {
        let Some(sender) = self.sender.as_ref() else {
            self.publish_failure(
                phase,
                operation_id,
                OperationalErrorKind::UnsupportedCapability,
            );
            return;
        };
        if sender.send(command).is_err() {
            self.publish_failure(
                phase,
                operation_id,
                OperationalErrorKind::CommandDispatchFailed,
            );
        }
    }

    fn publish_failure(
        &self,
        phase: &'static str,
        operation_id: Option<&str>,
        kind: OperationalErrorKind,
    ) {
        let _ = self
            .agent_events
            .publish_command_failure(NativeHostCommandFailureEvent {
                controller_lease_id: self.controller_lease_id.clone(),
                operation: OperationFailedOperation::Telemetry,
                operation_id: operation_id.map(str::to_owned),
                phase,
                kind,
                occurred_at_epoch_ms: 1,
            });
    }
}

impl RecordingTelemetrySource for TelemetryWorkerCommandSink {
    fn bind_controller_lease(&mut self, controller_lease_id: &str) {
        self.controller_lease_id = Some(controller_lease_id.to_owned());
    }

    fn configure_recording(&mut self, policy: RecordingTelemetryPolicy) {
        self.policy = policy;
    }

    fn start_recording_with_policy(
        &mut self,
        policy: RecordingTelemetryPolicy,
        recording_id: &str,
        started_at_monotonic_ns: u64,
    ) {
        self.policy = policy;
        self.enqueue_required(
            WorkerCommand::StartRecordingTelemetry {
                policy,
                recording_id: recording_id.to_owned(),
                started_at_monotonic_ns,
            },
            "start",
            Some(recording_id),
        );
    }

    fn start_recording(&mut self, recording_id: &str, started_at_monotonic_ns: u64) {
        self.enqueue_required(
            WorkerCommand::StartRecordingTelemetry {
                policy: self.policy,
                recording_id: recording_id.to_owned(),
                started_at_monotonic_ns,
            },
            "start",
            Some(recording_id),
        );
    }

    fn pause_recording(&mut self, recording_id: &str, paused_at_monotonic_ns: u64) {
        self.enqueue(
            WorkerCommand::PauseRecordingTelemetry {
                recording_id: recording_id.to_owned(),
                paused_at_monotonic_ns,
            },
            "pause",
            Some(recording_id),
        );
    }

    fn resume_recording(&mut self, recording_id: &str, resumed_at_monotonic_ns: u64) {
        self.enqueue(
            WorkerCommand::ResumeRecordingTelemetry {
                recording_id: recording_id.to_owned(),
                resumed_at_monotonic_ns,
            },
            "resume",
            Some(recording_id),
        );
    }

    fn finish_recording(&mut self, recording_id: &str, stopped_at_monotonic_ns: u64) {
        self.enqueue(
            WorkerCommand::FinishRecordingTelemetry {
                recording_id: recording_id.to_owned(),
                stopped_at_monotonic_ns,
            },
            "finish",
            Some(recording_id),
        );
    }

    fn cancel_recording(&mut self, recording_id: &str, cancelled_at_monotonic_ns: u64) {
        self.enqueue(
            WorkerCommand::CancelRecordingTelemetry {
                recording_id: recording_id.to_owned(),
                cancelled_at_monotonic_ns,
            },
            "cancel",
            Some(recording_id),
        );
    }

    fn discard_recording(&mut self, recording_id: &str) {
        self.enqueue(
            WorkerCommand::DiscardRecordingTelemetry {
                recording_id: recording_id.to_owned(),
            },
            "discard",
            Some(recording_id),
        );
    }

    fn snapshot_for_recording(
        &mut self,
        recording_id: &str,
        duration_ms: u64,
    ) -> Option<SanitizedRecordingTelemetrySnapshot> {
        let (result_sender, result_receiver) = std::sync::mpsc::sync_channel(1);
        self.enqueue(
            WorkerCommand::SnapshotRecordingTelemetry {
                recording_id: recording_id.to_owned(),
                duration_ms,
                result_sender,
            },
            "snapshot",
            Some(recording_id),
        );
        match result_receiver.recv_timeout(RUNTIME_APPLY_ACK_TIMEOUT) {
            Ok(snapshot) => snapshot,
            Err(RecvTimeoutError::Timeout) => {
                self.publish_failure(
                    "snapshot-timeout",
                    Some(recording_id),
                    OperationalErrorKind::CommandDispatchFailed,
                );
                None
            }
            Err(RecvTimeoutError::Disconnected) => None,
        }
    }
}

pub(super) fn run_telemetry_worker(
    component: AgentComponent,
    event_sender: &SyncSender<WorkerEvent>,
    command_receiver: &Receiver<WorkerCommand>,
    mut source: Box<dyn RecordingTelemetrySource>,
) {
    let _ = event_sender.send(WorkerEvent::Started(component));
    loop {
        match command_receiver.recv() {
            Ok(WorkerCommand::Stop) | Err(_) => break,
            Ok(WorkerCommand::FenceMode { result_sender }) => {
                let _ = result_sender.send(false);
            }
            Ok(WorkerCommand::StartRecordingTelemetry {
                policy,
                recording_id,
                started_at_monotonic_ns,
            }) => {
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
                | WorkerCommand::RestoreHotkeys { .. }
                | WorkerCommand::ApplyTrayRegistry { .. }
                | WorkerCommand::ShowTrayNotice { .. }
                | WorkerCommand::UpdateTrayRecordingStatus { .. }
                | WorkerCommand::RefreshTrayConnectionStatus
                | WorkerCommand::CaptureScreenshot { .. }
                | WorkerCommand::StartRecording { .. }
                | WorkerCommand::ControlRecording { .. }
                | WorkerCommand::ControlActiveRecording { .. }
                | WorkerCommand::PrepareManualAsset { .. }
                | WorkerCommand::PublishManualAsset { .. },
            ) => {}
            #[cfg(test)]
            Ok(WorkerCommand::TrayMenuCommandSelected { .. }) => {}
        }
    }
}

#[cfg(test)]
mod tests;
