//! Resident recording worker command broker.

mod lifecycle;
mod logging;
mod manual_asset;
mod request;
mod status;

use std::sync::mpsc::{Receiver, SyncSender, TrySendError};

use super::{AgentComponent, AgentEventHub, WorkerCommand, WorkerEvent};
use crate::{
    app::responses::{NativeHostCommandOutcome, NativeHostSideEffectError},
    protocol::{ExtensionRecordingControlCommandMessage, ExtensionRecordingStartCommandMessage},
    recording::{
        RecordingCommandEvent, RecordingRunner, RecordingRuntimeError, RecordingStartPolicy,
        RecordingStartRequest,
    },
};
use lifecycle::{
    ActiveRecording, RecordingLifecycleContext, reject_start_while_active, run_control_command,
    run_start_command,
};
use request::{extension_control_request, extension_start_request};

#[derive(Clone, Debug)]
pub(super) struct RecordingWorkerCommandSink {
    sender: Option<SyncSender<WorkerCommand>>,
}

impl RecordingWorkerCommandSink {
    pub(super) fn new(sender: Option<SyncSender<WorkerCommand>>) -> Self {
        Self { sender }
    }

    pub(super) fn start_recording(
        &mut self,
        command: &ExtensionRecordingStartCommandMessage,
        policy: RecordingStartPolicy,
    ) -> Result<NativeHostCommandOutcome, NativeHostSideEffectError> {
        self.enqueue(WorkerCommand::StartRecording {
            request: Box::new(extension_start_request(command)),
            policy,
        })
    }

    pub(super) fn control_recording(
        &mut self,
        command: &ExtensionRecordingControlCommandMessage,
    ) -> Result<NativeHostCommandOutcome, NativeHostSideEffectError> {
        self.enqueue(WorkerCommand::ControlRecording {
            request: Box::new(extension_control_request(command)),
        })
    }

    fn enqueue(
        &self,
        command: WorkerCommand,
    ) -> Result<NativeHostCommandOutcome, NativeHostSideEffectError> {
        let sender = self
            .sender
            .as_ref()
            .ok_or(NativeHostSideEffectError::UnsupportedCapability)?;
        match sender.try_send(command) {
            Ok(()) => Ok(NativeHostCommandOutcome::Accepted),
            Err(TrySendError::Full(_)) => Err(NativeHostSideEffectError::WorkerQueueFull),
            Err(TrySendError::Disconnected(_)) => Err(NativeHostSideEffectError::WorkerStopped),
        }
    }
}

pub(super) fn start_media_runner(
    runner: &mut dyn RecordingRunner,
    request: &RecordingStartRequest,
    policy: RecordingStartPolicy,
) -> Result<Vec<RecordingCommandEvent>, RecordingRuntimeError> {
    runner.start_recording(request, policy)
}

pub(super) struct RecordingWorkerInputs<'a> {
    pub(super) event_sender: &'a SyncSender<WorkerEvent>,
    pub(super) command_receiver: &'a Receiver<WorkerCommand>,
    pub(super) runner: Option<Box<dyn RecordingRunner>>,
    pub(super) agent_events: &'a AgentEventHub,
    pub(super) tray_command_sender: Option<&'a SyncSender<WorkerCommand>>,
    pub(super) tray_recording_state: &'a super::tray::TrayRecordingState,
    pub(super) asset_sender: Option<&'a SyncSender<WorkerCommand>>,
    pub(super) log: Option<&'a super::AgentProcessLogSink>,
}

pub(super) fn run_recording_worker(component: AgentComponent, inputs: RecordingWorkerInputs<'_>) {
    let _ = inputs.event_sender.send(WorkerEvent::Started(component));
    let command_receiver = inputs.command_receiver;
    let mut runtime = RecordingWorkerRuntime::new(inputs);
    while let Ok(command) = command_receiver.recv() {
        if !runtime.handle_command(command) {
            break;
        }
    }
}

struct RecordingWorkerRuntime<'a> {
    runner: Option<Box<dyn RecordingRunner>>,
    agent_events: &'a AgentEventHub,
    tray_command_sender: Option<&'a SyncSender<WorkerCommand>>,
    tray_recording_state: &'a super::tray::TrayRecordingState,
    asset_sender: Option<&'a SyncSender<WorkerCommand>>,
    log: Option<&'a super::AgentProcessLogSink>,
    accepted_since_fence: bool,
    active_recording: Option<ActiveRecording>,
}

impl<'a> RecordingWorkerRuntime<'a> {
    fn new(inputs: RecordingWorkerInputs<'a>) -> Self {
        Self {
            runner: inputs.runner,
            agent_events: inputs.agent_events,
            tray_command_sender: inputs.tray_command_sender,
            tray_recording_state: inputs.tray_recording_state,
            asset_sender: inputs.asset_sender,
            log: inputs.log,
            accepted_since_fence: false,
            active_recording: None,
        }
    }

    fn handle_command(&mut self, command: WorkerCommand) -> bool {
        match command {
            WorkerCommand::Stop => return false,
            WorkerCommand::FenceMode { result_sender } => {
                let _ = result_sender.send(self.accepted_since_fence);
                self.accepted_since_fence = false;
            }
            WorkerCommand::StartRecording { request, policy } => self.start(&request, policy),
            WorkerCommand::ControlRecording { request } => self.control(&request),
            WorkerCommand::ControlActiveRecording { control } => self.control_active(control),
            command => ignore_non_recording_command(&command),
        }
        true
    }

    fn start(&mut self, request: &RecordingStartRequest, policy: RecordingStartPolicy) {
        self.accepted_since_fence = true;
        if self.active_recording.is_some() {
            reject_start_while_active(self.agent_events, request);
            return;
        }
        let context = RecordingLifecycleContext {
            agent_events: self.agent_events,
            tray_command_sender: self.tray_command_sender,
            tray_recording_state: self.tray_recording_state,
            asset_sender: self.asset_sender,
            log: self.log,
        };
        self.active_recording = run_start_command(request, policy, &mut self.runner, context);
    }

    fn control(&mut self, request: &crate::recording::RecordingControlRequest) {
        self.accepted_since_fence = true;
        if self.run_control(request) && is_terminal_control(&request.control) {
            self.active_recording = None;
        }
    }

    fn control_active(&mut self, control: crate::protocol::RecordingControl) {
        let Some(active) = self.active_recording.as_ref() else {
            return;
        };
        let request = active.control_request(control);
        if self.run_control(&request) && is_terminal_control(&request.control) {
            self.active_recording = None;
        }
    }

    fn run_control(&mut self, request: &crate::recording::RecordingControlRequest) -> bool {
        let context = RecordingLifecycleContext {
            agent_events: self.agent_events,
            tray_command_sender: self.tray_command_sender,
            tray_recording_state: self.tray_recording_state,
            asset_sender: self.asset_sender,
            log: self.log,
        };
        run_control_command(request, &mut self.runner, context)
    }
}

const fn is_terminal_control(control: &crate::protocol::RecordingControl) -> bool {
    matches!(
        control,
        crate::protocol::RecordingControl::Stop | crate::protocol::RecordingControl::Cancel
    )
}

fn ignore_non_recording_command(command: &WorkerCommand) {
    match command {
        WorkerCommand::ApplyHotkeyPlan { .. }
        | WorkerCommand::ManualHotkeyAction { .. }
        | WorkerCommand::RestoreHotkeys { .. }
        | WorkerCommand::ApplyTrayRegistry { .. }
        | WorkerCommand::ShowTrayNotice { .. }
        | WorkerCommand::UpdateTrayRecordingStatus { .. }
        | WorkerCommand::RefreshTrayConnectionStatus
        | WorkerCommand::CaptureScreenshot { .. }
        | WorkerCommand::StartRecordingTelemetry { .. }
        | WorkerCommand::PauseRecordingTelemetry { .. }
        | WorkerCommand::ResumeRecordingTelemetry { .. }
        | WorkerCommand::FinishRecordingTelemetry { .. }
        | WorkerCommand::CancelRecordingTelemetry { .. }
        | WorkerCommand::DiscardRecordingTelemetry { .. }
        | WorkerCommand::SnapshotRecordingTelemetry { .. }
        | WorkerCommand::PrepareManualAsset { .. }
        | WorkerCommand::PublishManualAsset { .. } => {}
        #[cfg(test)]
        WorkerCommand::TrayMenuCommandSelected { .. } => {}
        WorkerCommand::Stop
        | WorkerCommand::FenceMode { .. }
        | WorkerCommand::StartRecording { .. }
        | WorkerCommand::ControlRecording { .. }
        | WorkerCommand::ControlActiveRecording { .. } => {
            unreachable!("recording command routed to non-recording branch")
        }
    }
}

#[cfg(test)]
mod tests;
