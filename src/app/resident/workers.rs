//! Resident agent worker startup and shutdown orchestration.

use std::sync::mpsc::{self, Receiver, SyncSender};

use super::{
    AgentComponent, AgentEventHub, AgentProcessLogSink, AgentRunError, AgentStartupStatus,
    WORKER_START_TIMEOUT, WorkerCommand, WorkerEvent,
    tray::{TrayConnectionState, TrayRecordingState},
    worker_start::{WorkerHandle, WorkerSenderRefs, WorkerStartContext, start_worker},
};

pub(super) fn start_workers(
    components: &[AgentComponent],
    startup_status: AgentStartupStatus,
    active_settings: &crate::settings::ActiveSettings,
    event_sender: &SyncSender<WorkerEvent>,
    shutdown_sender: &SyncSender<super::AgentShutdownSignal>,
    log: Option<&AgentProcessLogSink>,
) -> Vec<WorkerHandle> {
    let agent_events = AgentEventHub::default();
    let channels = components
        .iter()
        .copied()
        .map(WorkerChannel::new)
        .collect::<Vec<_>>();
    let senders = WorkerCommandSenders::from_channels(&channels);
    let mut workers = Vec::with_capacity(components.len());
    for channel in channels {
        let context = WorkerStartContext {
            startup_status,
            active_settings: active_settings.clone(),
            event_sender: event_sender.clone(),
            agent_events: agent_events.clone(),
            shutdown_sender: shutdown_sender.clone(),
        };
        let worker = start_worker(
            channel.component,
            channel.command_sender.clone(),
            channel.command_receiver,
            senders.as_refs(),
            context,
            log,
        );
        workers.push(worker);
    }
    workers
}

struct WorkerChannel {
    component: AgentComponent,
    command_sender: SyncSender<WorkerCommand>,
    command_receiver: Receiver<WorkerCommand>,
}

impl WorkerChannel {
    fn new(component: AgentComponent) -> Self {
        let (command_sender, command_receiver) = mpsc::sync_channel(1);
        Self {
            component,
            command_sender,
            command_receiver,
        }
    }
}

struct WorkerCommandSenders {
    tray: Option<SyncSender<WorkerCommand>>,
    pipe: Option<SyncSender<WorkerCommand>>,
    hotkey: Option<SyncSender<WorkerCommand>>,
    capture: Option<SyncSender<WorkerCommand>>,
    recording: Option<SyncSender<WorkerCommand>>,
    telemetry: Option<SyncSender<WorkerCommand>>,
    assets: Option<SyncSender<WorkerCommand>>,
    tray_connection: TrayConnectionState,
    tray_recording: TrayRecordingState,
}

impl WorkerCommandSenders {
    fn from_channels(channels: &[WorkerChannel]) -> Self {
        Self {
            tray: sender_for(channels, AgentComponent::Tray),
            pipe: sender_for(channels, AgentComponent::PipeServer),
            hotkey: sender_for(channels, AgentComponent::Hotkeys),
            capture: sender_for(channels, AgentComponent::CaptureWorker),
            recording: sender_for(channels, AgentComponent::EncoderWorker),
            telemetry: sender_for(channels, AgentComponent::TelemetryWorker),
            assets: sender_for(channels, AgentComponent::AssetPublisher),
            tray_connection: TrayConnectionState::default(),
            tray_recording: TrayRecordingState::default(),
        }
    }

    fn as_refs(&self) -> WorkerSenderRefs<'_> {
        WorkerSenderRefs {
            tray: self.tray.as_ref(),
            pipe: self.pipe.as_ref(),
            hotkey: self.hotkey.as_ref(),
            capture: self.capture.as_ref(),
            recording: self.recording.as_ref(),
            telemetry: self.telemetry.as_ref(),
            assets: self.assets.as_ref(),
            tray_connection: &self.tray_connection,
            tray_recording: &self.tray_recording,
        }
    }
}

fn sender_for(
    channels: &[WorkerChannel],
    component: AgentComponent,
) -> Option<SyncSender<WorkerCommand>> {
    channels
        .iter()
        .find(|channel| channel.component == component)
        .map(|channel| channel.command_sender.clone())
}

pub(super) fn wait_for_worker_starts(
    components: &[AgentComponent],
    event_receiver: &Receiver<WorkerEvent>,
) -> Result<Vec<AgentComponent>, AgentRunError> {
    let mut started = Vec::with_capacity(components.len());
    while started.len() < components.len() {
        let event = event_receiver
            .recv_timeout(WORKER_START_TIMEOUT)
            .map_err(|_| worker_start_error(components, &started))?;
        match event {
            WorkerEvent::Started(component) if components.contains(&component) => {
                started.push(component);
            }
            WorkerEvent::Failed(component) if components.contains(&component) => {
                return Err(AgentRunError::StartFailed(component));
            }
            WorkerEvent::Started(_) | WorkerEvent::Failed(_) => {
                return Err(AgentRunError::EventChannelClosed);
            }
        }
    }
    Ok(components.to_vec())
}

fn worker_start_error(components: &[AgentComponent], started: &[AgentComponent]) -> AgentRunError {
    let component = components
        .iter()
        .copied()
        .find(|component| !started.contains(component))
        .unwrap_or(AgentComponent::PipeServer);
    AgentRunError::StartTimeout(component)
}

pub(super) fn stop_workers(
    workers: &mut [WorkerHandle],
) -> Result<Vec<AgentComponent>, AgentRunError> {
    let mut stopped = Vec::with_capacity(workers.len());
    for worker in workers.iter_mut().rev() {
        worker
            .command_sender
            .send(WorkerCommand::Stop)
            .map_err(|_| AgentRunError::CommandChannelClosed(worker.component))?;
        let join_handle = worker
            .join_handle
            .take()
            .ok_or(AgentRunError::JoinFailed(worker.component))?;
        join_handle
            .join()
            .map_err(|_| AgentRunError::JoinFailed(worker.component))?;
        stopped.push(worker.component);
    }
    Ok(stopped)
}
