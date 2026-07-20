//! Tray-worker lifecycle and command-polling loop.

use std::{
    sync::mpsc::{Receiver, RecvTimeoutError, SyncSender},
    time::Duration,
};

#[cfg(not(windows))]
use crate::tray::NoopTrayCommandSource;
use crate::tray::TrayCommandSource;
#[cfg(windows)]
use crate::tray::runtime::WindowsTrayCommandSource;

use super::super::{
    AgentComponent, AgentEventHub, AgentShutdownSignal, AgentStartupStatus, WorkerCommand,
    WorkerEvent,
};
use super::{
    ManualActionDispatcher, ManualHotkeyApplyError, ManualWorkerSenders, TrayWorker,
    view::TrayWorkerControl,
};

const TRAY_EVENT_POLL_INTERVAL: Duration = Duration::from_millis(25);

#[cfg(test)]
#[path = "runner/tests.rs"]
mod tests;

pub(crate) struct TrayWorkerInputs<'a> {
    pub(crate) startup_status: AgentStartupStatus,
    pub(crate) event_sender: &'a SyncSender<WorkerEvent>,
    pub(crate) command_receiver: &'a Receiver<WorkerCommand>,
    pub(crate) events: &'a AgentEventHub,
    pub(crate) shutdown_sender: &'a SyncSender<AgentShutdownSignal>,
    pub(crate) active_settings: crate::settings::ActiveSettings,
    pub(crate) manual_dispatcher: Option<ManualActionDispatcher>,
    pub(crate) manual_senders: ManualWorkerSenders,
    pub(crate) connection_state: super::TrayConnectionState,
    pub(crate) recording_state: super::TrayRecordingState,
    pub(crate) log: Option<&'a super::super::AgentProcessLogSink>,
}

struct TrayWorkerRuntime<'a> {
    events: &'a AgentEventHub,
    shutdown_sender: &'a SyncSender<AgentShutdownSignal>,
    active_settings: crate::settings::ActiveSettings,
    manual_dispatcher: Option<ManualActionDispatcher>,
    manual_senders: ManualWorkerSenders,
    connection_state: super::TrayConnectionState,
    recording_state: super::TrayRecordingState,
}

pub(crate) fn run_tray_worker(component: AgentComponent, inputs: TrayWorkerInputs<'_>) {
    #[cfg(windows)]
    let mut source = match WindowsTrayCommandSource::new() {
        Ok(source) => source,
        Err(_) => {
            log_tray_startup_failure(
                inputs.log,
                "source-create",
                "tray.source_create_failed",
                "tray command source creation failed",
            );
            let _ = inputs.event_sender.send(WorkerEvent::Failed(component));
            return;
        }
    };
    #[cfg(not(windows))]
    let mut source = NoopTrayCommandSource;
    run_tray_worker_with_source(
        component,
        inputs.startup_status,
        inputs.event_sender,
        inputs.command_receiver,
        inputs.log,
        TrayWorkerRuntime {
            events: inputs.events,
            shutdown_sender: inputs.shutdown_sender,
            active_settings: inputs.active_settings,
            manual_dispatcher: inputs.manual_dispatcher,
            manual_senders: inputs.manual_senders,
            connection_state: inputs.connection_state,
            recording_state: inputs.recording_state,
        },
        &mut source,
    );
}

fn run_tray_worker_with_source(
    component: AgentComponent,
    startup_status: AgentStartupStatus,
    event_sender: &SyncSender<WorkerEvent>,
    command_receiver: &Receiver<WorkerCommand>,
    log: Option<&super::super::AgentProcessLogSink>,
    runtime: TrayWorkerRuntime<'_>,
    source: &mut impl TrayCommandSource,
) {
    let mut worker = TrayWorker::with_projection_states(
        startup_status,
        runtime.active_settings,
        runtime.manual_dispatcher,
        runtime.manual_senders,
        runtime.connection_state,
        runtime.recording_state,
    );
    if let Err(error) =
        activate_startup_manual_hotkeys(&worker.active_settings, &worker.manual_senders)
    {
        log_tray_startup_failure(
            log,
            "manual-hotkeys",
            "tray.manual_hotkeys_failed",
            &error.to_string(),
        );
        let _ = event_sender.send(WorkerEvent::Failed(component));
        return;
    }
    if worker.render_fallback_menu(source).is_err() {
        log_tray_startup_failure(
            log,
            "fallback-render",
            "tray.fallback_render_failed",
            "tray fallback render failed",
        );
        let _ = event_sender.send(WorkerEvent::Failed(component));
        return;
    }
    let _ = event_sender.send(WorkerEvent::Started(component));
    loop {
        match command_receiver.recv_timeout(TRAY_EVENT_POLL_INTERVAL) {
            Ok(command) => {
                if worker.handle_command(command, runtime.events, runtime.shutdown_sender, source)
                    == TrayWorkerControl::Stop
                {
                    break;
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        worker.sync_connection_status(source);
        worker.sync_recording_status(runtime.shutdown_sender, source);
        let source_events = TrayWorker::drain_source_events(source);
        if reconcile_queued_commands(
            &mut worker,
            command_receiver,
            runtime.events,
            runtime.shutdown_sender,
            source,
        ) == TrayWorkerControl::Stop
        {
            break;
        }
        worker.sync_connection_status(source);
        worker.sync_recording_status(runtime.shutdown_sender, source);
        worker.dispatch_source_events(
            source_events,
            runtime.events,
            runtime.shutdown_sender,
            source,
        );
    }
}

#[cfg(all(windows, not(test)))]
fn log_tray_startup_failure(
    log: Option<&super::super::AgentProcessLogSink>,
    phase: &'static str,
    code: &'static str,
    message: &str,
) {
    let Some(log) = log else { return };
    let error = crate::diagnostics::LogErrorFields {
        code: code.to_owned(),
        recoverable: true,
    };
    let _ = log.write_event_now(crate::diagnostics::ProcessLogEvent {
        level: crate::diagnostics::LogLevel::Warn,
        target: "sniptale_native_agent::tray",
        operation: "startup",
        operation_id: None,
        phase,
        message,
        duration_ms: None,
        error: Some(&error),
    });
}

#[cfg(any(not(windows), test))]
fn log_tray_startup_failure(
    _log: Option<&super::super::AgentProcessLogSink>,
    _phase: &'static str,
    _code: &'static str,
    _message: &str,
) {
}

fn activate_startup_manual_hotkeys(
    active_settings: &crate::settings::ActiveSettings,
    manual_senders: &ManualWorkerSenders,
) -> Result<(), ManualHotkeyApplyError> {
    let profile = active_settings
        .profile_snapshot()
        .map_err(|_| ManualHotkeyApplyError::ProfileUnavailable)?;
    if profile.operating_mode() == crate::settings::OperatingMode::Manual
        && let Some(hotkeys) = profile.manual_hotkeys()
    {
        manual_senders.apply_manual_hotkeys(&hotkeys)?;
    }
    Ok(())
}

pub(super) fn reconcile_queued_commands(
    worker: &mut TrayWorker,
    command_receiver: &Receiver<WorkerCommand>,
    events: &AgentEventHub,
    shutdown_sender: &SyncSender<AgentShutdownSignal>,
    source: &mut impl TrayCommandSource,
) -> TrayWorkerControl {
    while let Ok(command) = command_receiver.try_recv() {
        if worker.handle_command(command, events, shutdown_sender, source)
            == TrayWorkerControl::Stop
        {
            return TrayWorkerControl::Stop;
        }
    }
    TrayWorkerControl::Continue
}
