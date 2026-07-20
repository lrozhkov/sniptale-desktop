//! Windows resident pipe worker glue for the tray agent.

#[cfg(windows)]
use std::{
    sync::mpsc::{Receiver, RecvTimeoutError, SyncSender, TryRecvError},
    time::Duration,
};

#[cfg(windows)]
use crate::app::resident::{pipe_broker::AgentPipeServerControl, pipe_windows as platform_pipe};

#[cfg(windows)]
use super::AgentEventHub;
#[cfg(windows)]
use super::AgentProcessLogSink;
#[cfg(windows)]
use super::{
    AgentComponent, WorkerCommand, WorkerEvent, capture_worker, effects, hotkeys, recording_worker,
    tray,
};

#[cfg(windows)]
pub(super) fn run_windows_pipe_server_worker(
    component: AgentComponent,
    event_sender: SyncSender<WorkerEvent>,
    command_receiver: Receiver<WorkerCommand>,
    tray_command_sender: Option<SyncSender<WorkerCommand>>,
    hotkey_command_sender: Option<SyncSender<WorkerCommand>>,
    capture_command_sender: Option<SyncSender<WorkerCommand>>,
    recording_command_sender: Option<SyncSender<WorkerCommand>>,
    tray_connection: tray::TrayConnectionState,
    active_settings: crate::settings::ActiveSettings,
    agent_events: AgentEventHub,
    log: Option<AgentProcessLogSink>,
) {
    tracing::info!(
        target: "sniptale_native_agent::resident_pipe",
        "resident pipe worker started"
    );
    log_pipe_worker_event(
        log.as_ref(),
        "pipe-worker-started",
        "resident pipe worker started",
        None,
    );
    let _ = event_sender.send(WorkerEvent::Started(component));
    let screenshot_store = open_screenshot_store_for_pipe(log.as_ref());
    let recording_store = open_recording_store_for_pipe(log.as_ref());
    let effects_factory = {
        let hotkey_command_sender = hotkey_command_sender.clone();
        let tray_command_sender = tray_command_sender.clone();
        let capture_command_sender = capture_command_sender.clone();
        let recording_command_sender = recording_command_sender.clone();
        let active_settings = active_settings.clone();
        move || {
            effects::AgentSideEffects::new_with_active_settings(
                hotkeys::HotkeyWorkerPlanSink::new(hotkey_command_sender.clone()),
                tray::TrayWorkerRegistrySink::with_connection_state(
                    tray_command_sender.clone(),
                    tray_connection.clone(),
                ),
                screenshot_store.clone(),
                capture_worker::CaptureWorkerCommandSink::new(capture_command_sender.clone()),
                recording_store.clone(),
                recording_worker::RecordingWorkerCommandSink::new(recording_command_sender.clone()),
                Some(active_settings.clone()),
            )
        }
    };
    let mut stop_requested = false;
    while !stop_requested {
        if !active_settings.allows_extension_ingress() {
            stop_requested = wait_for_mode_change_or_stop(&command_receiver);
            continue;
        }
        let Ok(mut server) = platform_pipe::WindowsAgentPipeServer::new(active_settings.clone())
        else {
            log_pipe_worker_event(
                log.as_ref(),
                "pipe-worker-init",
                "resident pipe integration is unavailable",
                Some("resident_pipe.worker_init_failed"),
            );
            stop_requested = wait_for_mode_change_or_stop(&command_receiver);
            continue;
        };
        stop_requested = run_active_pipe_server(
            &mut server,
            &command_receiver,
            &active_settings,
            effects_factory.clone(),
            agent_events.clone(),
            log.clone(),
        );
    }
}

#[cfg(windows)]
fn run_active_pipe_server<E>(
    server: &mut platform_pipe::WindowsAgentPipeServer,
    command_receiver: &Receiver<WorkerCommand>,
    active_settings: &crate::settings::ActiveSettings,
    effects_factory: E,
    agent_events: AgentEventHub,
    log: Option<AgentProcessLogSink>,
) -> bool
where
    E: crate::app::resident::pipe_broker::AgentPipeSessionEffectsFactory,
{
    let mut stop_requested = false;
    let mut fence_sender = None;
    match server.run_with_effects_events_and_log(
        effects_factory,
        &agent_events,
        log.as_ref(),
        |_| match command_receiver.try_recv() {
            Ok(WorkerCommand::Stop) | Err(TryRecvError::Disconnected) => {
                stop_requested = true;
                AgentPipeServerControl::Stop
            }
            Ok(WorkerCommand::FenceMode { result_sender }) => {
                fence_sender = Some(result_sender);
                AgentPipeServerControl::Stop
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
            ) => AgentPipeServerControl::Continue,
            #[cfg(test)]
            Ok(WorkerCommand::TrayMenuCommandSelected { .. }) => AgentPipeServerControl::Continue,
            Err(TryRecvError::Empty) if !active_settings.allows_extension_ingress() => {
                AgentPipeServerControl::Stop
            }
            Err(TryRecvError::Empty) => AgentPipeServerControl::Continue,
        },
    ) {
        Ok(report) => tracing::info!(
            target: "sniptale_native_agent::resident_pipe",
            accepted = report.accepted,
            completed = report.completed,
            failed = report.failed,
            "resident pipe worker stopped"
        ),
        Err(error) => tracing::warn!(
            target: "sniptale_native_agent::resident_pipe",
            "resident pipe worker failed: {error}"
        ),
    }
    if let Some(result_sender) = fence_sender {
        let _ = result_sender.send(false);
    }
    stop_requested
}

#[cfg(windows)]
fn wait_for_mode_change_or_stop(command_receiver: &Receiver<WorkerCommand>) -> bool {
    match command_receiver.recv_timeout(Duration::from_millis(100)) {
        Ok(WorkerCommand::Stop) | Err(RecvTimeoutError::Disconnected) => true,
        Ok(WorkerCommand::FenceMode { result_sender }) => {
            let _ = result_sender.send(false);
            false
        }
        Ok(_) | Err(RecvTimeoutError::Timeout) => false,
    }
}

#[cfg(windows)]
fn open_screenshot_store_for_pipe(
    log: Option<&AgentProcessLogSink>,
) -> Option<crate::capture::ScreenshotTransferStore> {
    match crate::capture::ScreenshotTransferStore::open_default() {
        Ok(store) => Some(store),
        Err(_) => {
            tracing::warn!(
                target: "sniptale_native_agent::resident_pipe",
                "screenshot transfer store open failed"
            );
            log_pipe_worker_event(
                log,
                "screenshot-store-open",
                "screenshot transfer store open failed",
                Some("resident_pipe.screenshot_store_open_failed"),
            );
            None
        }
    }
}

#[cfg(windows)]
fn open_recording_store_for_pipe(
    log: Option<&AgentProcessLogSink>,
) -> Option<crate::recording::RecordingTransferStore> {
    match crate::recording::RecordingTransferStore::open_default() {
        Ok(store) => Some(store),
        Err(_) => {
            tracing::warn!(
                target: "sniptale_native_agent::resident_pipe",
                "recording transfer store open failed"
            );
            log_pipe_worker_event(
                log,
                "recording-store-open",
                "recording transfer store open failed",
                Some("resident_pipe.recording_store_open_failed"),
            );
            None
        }
    }
}

#[cfg(all(windows, not(test)))]
fn log_pipe_worker_event(
    log: Option<&AgentProcessLogSink>,
    phase: &'static str,
    message: &str,
    code: Option<&str>,
) {
    let Some(log) = log else {
        return;
    };
    let error = code.map(|code| crate::diagnostics::LogErrorFields {
        code: code.to_owned(),
        recoverable: true,
    });
    let _ = log.write_event_now(crate::diagnostics::ProcessLogEvent {
        level: if error.is_some() {
            crate::diagnostics::LogLevel::Warn
        } else {
            crate::diagnostics::LogLevel::Info
        },
        target: "sniptale_native_agent::resident_pipe",
        operation: "startup",
        operation_id: None,
        phase,
        message,
        duration_ms: None,
        error: error.as_ref(),
    });
}

#[cfg(all(windows, test))]
fn log_pipe_worker_event(
    _log: Option<&AgentProcessLogSink>,
    _phase: &'static str,
    _message: &str,
    _code: Option<&str>,
) {
}
