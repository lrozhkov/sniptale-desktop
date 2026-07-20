//! Resident worker thread dispatch.
use super::{
    AgentComponent, AgentEventHub, AgentProcessLogSink, AgentShutdownSignal, AgentStartupStatus,
    WorkerCommand, WorkerEvent, assets_worker, audio_worker, capture_worker, hotkeys,
    recording_worker, telemetry_worker, transfer,
    worker_runtime::{default_capture_runner, default_recording_runner, default_telemetry_source},
};
use std::{
    sync::mpsc::{Receiver, SyncSender},
    thread::{self, JoinHandle},
};
#[path = "worker_start/basic.rs"]
mod basic;
pub(super) fn start_worker(
    component: AgentComponent,
    command_sender: SyncSender<WorkerCommand>,
    command_receiver: Receiver<WorkerCommand>,
    sender_refs: WorkerSenderRefs<'_>,
    context: WorkerStartContext,
    log: Option<&AgentProcessLogSink>,
) -> WorkerHandle {
    #[cfg(all(not(windows), not(test)))]
    let _ = log;
    #[cfg(all(windows, not(test)))]
    let log = log.cloned();
    #[cfg(test)]
    let log = log.copied();
    let senders = WorkerSenders {
        tray: sender_refs.tray.cloned(),
        pipe: sender_refs.pipe.cloned(),
        hotkey: sender_refs.hotkey.cloned(),
        capture: sender_refs.capture.cloned(),
        recording: sender_refs.recording.cloned(),
        telemetry: sender_refs.telemetry.cloned(),
        assets: sender_refs.assets.cloned(),
        tray_connection: sender_refs.tray_connection.clone(),
        tray_recording: sender_refs.tray_recording.clone(),
    };
    let join_handle = thread::spawn(move || {
        run_worker_thread(component, command_receiver, senders, context, log);
    });
    WorkerHandle {
        component,
        command_sender,
        join_handle: Some(join_handle),
    }
}
#[derive(Clone, Copy)]
pub(super) struct WorkerSenderRefs<'a> {
    pub(super) tray: Option<&'a SyncSender<WorkerCommand>>,
    pub(super) pipe: Option<&'a SyncSender<WorkerCommand>>,
    pub(super) hotkey: Option<&'a SyncSender<WorkerCommand>>,
    pub(super) capture: Option<&'a SyncSender<WorkerCommand>>,
    pub(super) recording: Option<&'a SyncSender<WorkerCommand>>,
    pub(super) telemetry: Option<&'a SyncSender<WorkerCommand>>,
    pub(super) assets: Option<&'a SyncSender<WorkerCommand>>,
    pub(super) tray_connection: &'a super::tray::TrayConnectionState,
    pub(super) tray_recording: &'a super::tray::TrayRecordingState,
}
pub(super) struct WorkerStartContext {
    pub(super) startup_status: AgentStartupStatus,
    pub(super) active_settings: crate::settings::ActiveSettings,
    pub(super) event_sender: SyncSender<WorkerEvent>,
    pub(super) agent_events: AgentEventHub,
    pub(super) shutdown_sender: SyncSender<AgentShutdownSignal>,
}
#[derive(Debug)]
pub(super) struct WorkerHandle {
    pub(super) component: AgentComponent,
    pub(super) command_sender: SyncSender<WorkerCommand>,
    pub(super) join_handle: Option<JoinHandle<()>>,
}
struct WorkerSenders {
    tray: Option<SyncSender<WorkerCommand>>,
    pipe: Option<SyncSender<WorkerCommand>>,
    hotkey: Option<SyncSender<WorkerCommand>>,
    capture: Option<SyncSender<WorkerCommand>>,
    recording: Option<SyncSender<WorkerCommand>>,
    telemetry: Option<SyncSender<WorkerCommand>>,
    assets: Option<SyncSender<WorkerCommand>>,
    tray_connection: super::tray::TrayConnectionState,
    tray_recording: super::tray::TrayRecordingState,
}

fn run_worker_thread(
    component: AgentComponent,
    command_receiver: Receiver<WorkerCommand>,
    senders: WorkerSenders,
    context: WorkerStartContext,
    log: Option<AgentProcessLogSink>,
) {
    #[cfg(not(windows))]
    let _ = context.active_settings.allows_extension_ingress();
    if run_core_worker(
        component,
        &command_receiver,
        &senders,
        &context,
        log.as_ref(),
    ) || run_media_worker(
        component,
        &command_receiver,
        &senders,
        &context,
        log.as_ref(),
    ) {
        return;
    }
    #[cfg(not(windows))]
    let _ = (
        &senders.tray,
        &senders.hotkey,
        &senders.capture,
        &senders.recording,
        &senders.telemetry,
    );
    #[cfg(windows)]
    if component == AgentComponent::PipeServer {
        super::pipe::run_windows_pipe_server_worker(
            component,
            context.event_sender,
            command_receiver,
            senders.tray,
            senders.hotkey,
            senders.capture,
            senders.recording,
            senders.tray_connection,
            context.active_settings,
            context.agent_events,
            log,
        );
        return;
    }
    basic::run_basic_worker(component, &context.event_sender, &command_receiver);
    #[cfg(not(windows))]
    let _ = (senders, context, command_receiver, log);
}

fn run_core_worker(
    component: AgentComponent,
    command_receiver: &Receiver<WorkerCommand>,
    senders: &WorkerSenders,
    context: &WorkerStartContext,
    log: Option<&AgentProcessLogSink>,
) -> bool {
    if component == AgentComponent::AssetPublisher {
        let service = assets_worker::default_asset_publish_service();
        assets_worker::run_asset_publisher_worker(
            component,
            &context.event_sender,
            command_receiver,
            service.as_ref(),
            senders.tray.as_ref(),
        );
        return true;
    }
    if component == AgentComponent::Tray {
        super::tray::run_tray_worker(
            component,
            super::tray::TrayWorkerInputs {
                startup_status: context.startup_status,
                event_sender: &context.event_sender,
                command_receiver,
                events: &context.agent_events,
                shutdown_sender: &context.shutdown_sender,
                active_settings: context.active_settings.clone(),
                manual_dispatcher: manual_dispatcher(&context.active_settings, senders),
                manual_senders: super::tray::ManualWorkerSenders::new(
                    senders.capture.clone(),
                    senders.recording.clone(),
                    senders.assets.clone(),
                    senders.hotkey.clone(),
                    senders.pipe.clone(),
                ),
                connection_state: senders.tray_connection.clone(),
                recording_state: senders.tray_recording.clone(),
                log,
            },
        );
        return true;
    }
    if component == AgentComponent::Hotkeys {
        #[cfg(windows)]
        let mut registrar = crate::hotkeys::WindowsHotkeyRegistrar::default();
        #[cfg(not(windows))]
        let mut registrar = hotkeys::NoopHotkeyRegistrar;
        hotkeys::run_hotkey_worker(
            component,
            &context.event_sender,
            command_receiver,
            &mut registrar,
            &context.agent_events,
            senders.tray.as_ref(),
            #[cfg(windows)]
            log,
            #[cfg(not(windows))]
            None,
        );
        return true;
    }
    if component == AgentComponent::TransferWorker {
        transfer::run_transfer_worker(component, &context.event_sender, command_receiver, log);
        return true;
    }
    false
}

fn manual_dispatcher(
    settings: &crate::settings::ActiveSettings,
    senders: &WorkerSenders,
) -> Option<super::control::manual_dispatch::ManualActionDispatcher> {
    settings
        .epoch()
        .ok()
        .filter(|epoch| epoch.stable_mode() == Some(crate::settings::OperatingMode::Manual))
        .map(|_| {
            super::control::manual_dispatch::ManualActionDispatcher::new(
                settings.clone(),
                senders.capture.clone(),
                senders.recording.clone(),
                senders.assets.clone(),
            )
        })
}

fn run_media_worker(
    component: AgentComponent,
    command_receiver: &Receiver<WorkerCommand>,
    senders: &WorkerSenders,
    context: &WorkerStartContext,
    log: Option<&AgentProcessLogSink>,
) -> bool {
    if component == AgentComponent::CaptureWorker {
        capture_worker::run_capture_worker(
            component,
            &context.event_sender,
            command_receiver,
            default_capture_runner(log),
            &context.agent_events,
            senders.assets.as_ref(),
        );
        return true;
    }
    if component == AgentComponent::EncoderWorker {
        recording_worker::run_recording_worker(
            component,
            recording_worker::RecordingWorkerInputs {
                event_sender: &context.event_sender,
                command_receiver,
                runner: default_recording_runner(
                    log,
                    senders.telemetry.clone(),
                    context.agent_events.clone(),
                ),
                agent_events: &context.agent_events,
                tray_command_sender: senders.tray.as_ref(),
                tray_recording_state: &senders.tray_recording,
                asset_sender: senders.assets.as_ref(),
                log,
            },
        );
        return true;
    }
    if component == AgentComponent::TelemetryWorker {
        telemetry_worker::run_telemetry_worker(
            component,
            &context.event_sender,
            command_receiver,
            default_telemetry_source(),
        );
        return true;
    }
    if component == AgentComponent::AudioWorker {
        audio_worker::run_audio_worker(component, &context.event_sender, command_receiver);
        return true;
    }
    false
}
