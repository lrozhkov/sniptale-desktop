//! Recording-owner lifecycle helpers for direct and extension-originated controls.

use std::sync::mpsc::SyncSender;

use crate::{
    app::{
        resident::{AgentEventHub, WorkerCommand, command_events, tray::TrayRecordingState},
        responses::NativeHostCommandFailureEvent,
    },
    protocol::{OperationFailedOperation, OperationalErrorKind, RecordingControl},
    recording::{
        RecordingControlRequest, RecordingDeliverySession, RecordingRunner, RecordingRuntimeError,
        RecordingStartPolicy, RecordingStartRequest,
    },
    transfer::identity::derived_media_id,
};

use super::{
    logging::log_recording_runtime_failure, manual_asset::enqueue_manual_recording_publish,
    request::now_epoch_ms, start_media_runner, status::set_tray_recording_status,
};

#[derive(Clone, Debug)]
pub(super) struct ActiveRecording {
    operation_id: String,
    recording_id: String,
    delivery_session: RecordingDeliverySession,
}

impl ActiveRecording {
    fn from_start(request: &RecordingStartRequest) -> Self {
        Self {
            operation_id: request.operation_id.clone(),
            recording_id: derived_media_id("recording", &request.operation_id),
            delivery_session: request.delivery_session.clone(),
        }
    }

    pub(super) fn control_request(&self, control: RecordingControl) -> RecordingControlRequest {
        RecordingControlRequest {
            operation_id: self.operation_id.clone(),
            recording_id: self.recording_id.clone(),
            control,
            requested_at_epoch_ms: now_epoch_ms(),
            delivery_session: self.delivery_session.clone(),
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct RecordingLifecycleContext<'a> {
    pub(super) agent_events: &'a AgentEventHub,
    pub(super) tray_command_sender: Option<&'a SyncSender<WorkerCommand>>,
    pub(super) tray_recording_state: &'a TrayRecordingState,
    pub(super) asset_sender: Option<&'a SyncSender<WorkerCommand>>,
    pub(super) log: Option<&'a super::super::AgentProcessLogSink>,
}

pub(super) fn run_start_command(
    request: &RecordingStartRequest,
    policy: RecordingStartPolicy,
    runner: &mut Option<Box<dyn RecordingRunner>>,
    context: RecordingLifecycleContext<'_>,
) -> Option<ActiveRecording> {
    let RecordingLifecycleContext {
        agent_events,
        tray_command_sender,
        tray_recording_state,
        asset_sender,
        log,
    } = context;
    agent_events.desktop.begin(
        &request.operation_id,
        request.delivery_route.local_asset_job_id(),
        crate::tray::desktop::model::CaptureKind::Video,
        crate::tray::desktop::model::CapturePhase::Starting,
        Some(&derived_media_id("recording", &request.operation_id)),
    );
    agent_events.desktop.advance(
        &request.operation_id,
        crate::tray::desktop::model::CapturePhase::Starting,
    );
    set_tray_recording_status(
        tray_recording_state,
        tray_command_sender,
        crate::tray::TrayRecordingStatus::Preparing,
    );
    let Some(runner) = runner.as_mut() else {
        agent_events.desktop.fail(&request.operation_id);
        publish_start_failure(
            agent_events,
            request,
            OperationalErrorKind::UnsupportedCapability,
        );
        notify_manual_failure(request, tray_command_sender);
        set_tray_recording_status(
            tray_recording_state,
            tray_command_sender,
            crate::tray::TrayRecordingStatus::Idle,
        );
        return None;
    };
    match start_media_runner(runner.as_mut(), request, policy) {
        Ok(events) => {
            publish_recording_events(
                events,
                request.delivery_session.controller_lease_id.as_deref(),
                agent_events,
                asset_sender,
            );
            set_tray_recording_status(
                tray_recording_state,
                tray_command_sender,
                crate::tray::TrayRecordingStatus::Recording,
            );
            Some(ActiveRecording::from_start(request))
        }
        Err(error) => {
            agent_events.desktop.fail(&request.operation_id);
            log_recording_runtime_failure(log, "start-failed", &request.operation_id, error);
            publish_start_failure(agent_events, request, recording_error_kind(error));
            notify_manual_failure(request, tray_command_sender);
            set_tray_recording_status(
                tray_recording_state,
                tray_command_sender,
                crate::tray::TrayRecordingStatus::Idle,
            );
            None
        }
    }
}

fn notify_manual_failure(
    request: &RecordingStartRequest,
    tray_command_sender: Option<&SyncSender<WorkerCommand>>,
) {
    if request.delivery_session.controller_lease_id.is_some() {
        return;
    }
    if let Some(sender) = tray_command_sender {
        // Manual failures have no browser subscriber, but media control must not wait for the UI.
        // The notice is best-effort while the coalescing recording state remains authoritative.
        let _ = sender.try_send(WorkerCommand::ShowTrayNotice {
            kind: crate::tray::TrayNoticeKind::ActionRejected,
        });
    }
}

pub(super) fn reject_start_while_active(
    agent_events: &AgentEventHub,
    request: &RecordingStartRequest,
) {
    agent_events.desktop.reject_pending(&request.operation_id);
    publish_start_failure(
        agent_events,
        request,
        OperationalErrorKind::RecordingRuntimeFailed,
    );
}

pub(super) fn run_control_command(
    request: &RecordingControlRequest,
    runner: &mut Option<Box<dyn RecordingRunner>>,
    context: RecordingLifecycleContext<'_>,
) -> bool {
    let RecordingLifecycleContext {
        agent_events,
        tray_command_sender,
        tray_recording_state,
        asset_sender,
        log,
    } = context;
    let next_status = next_status(&request.control);
    if matches!(request.control, RecordingControl::Stop) {
        set_tray_recording_status(
            tray_recording_state,
            tray_command_sender,
            crate::tray::TrayRecordingStatus::Finalizing,
        );
    }
    let Some(runner) = runner.as_mut() else {
        publish_control_failure(
            agent_events,
            request,
            OperationalErrorKind::UnsupportedCapability,
        );
        set_tray_recording_status(
            tray_recording_state,
            tray_command_sender,
            crate::tray::TrayRecordingStatus::Idle,
        );
        return false;
    };
    match runner.control_recording(request) {
        Ok(events) => {
            publish_recording_events(
                events,
                request.delivery_session.controller_lease_id.as_deref(),
                agent_events,
                asset_sender,
            );
            if matches!(request.control, RecordingControl::Cancel) {
                agent_events
                    .desktop
                    .recording_discarded(&request.recording_id);
            }
            let status = if matches!(request.control, RecordingControl::Stop) {
                crate::tray::TrayRecordingStatus::Idle
            } else {
                next_status
            };
            set_tray_recording_status(tray_recording_state, tray_command_sender, status);
            true
        }
        Err(error) => {
            log_recording_runtime_failure(log, "control-failed", &request.operation_id, error);
            publish_control_failure(agent_events, request, recording_error_kind(error));
            set_tray_recording_status(
                tray_recording_state,
                tray_command_sender,
                crate::tray::TrayRecordingStatus::Idle,
            );
            false
        }
    }
}

const fn next_status(control: &RecordingControl) -> crate::tray::TrayRecordingStatus {
    match control {
        RecordingControl::Pause => crate::tray::TrayRecordingStatus::Paused,
        RecordingControl::Resume => crate::tray::TrayRecordingStatus::Recording,
        RecordingControl::Stop => crate::tray::TrayRecordingStatus::Finalizing,
        RecordingControl::Cancel => crate::tray::TrayRecordingStatus::Idle,
    }
}

/// Returns true when automatic finalization or an owner failure ends this worker's recording.
pub(super) fn run_progress_tick(
    active: &ActiveRecording,
    observed_epoch_ms: u64,
    runner: &mut Option<Box<dyn RecordingRunner>>,
    context: RecordingLifecycleContext<'_>,
) -> bool {
    let result = runner
        .as_mut()
        .ok_or(RecordingRuntimeError::RuntimeUnavailable)
        .and_then(|runner| runner.tick(observed_epoch_ms));
    match result {
        Ok(events) => {
            let terminal = events.iter().any(|event| match event {
                crate::recording::RecordingCommandEvent::ManualFinalized(event) => {
                    event.recording_id == active.recording_id
                }
                crate::recording::RecordingCommandEvent::Stopped(event) => {
                    event.pending.recording_id == active.recording_id
                }
                _ => false,
            });
            publish_recording_events(
                events,
                active.delivery_session.controller_lease_id.as_deref(),
                context.agent_events,
                context.asset_sender,
            );
            if terminal {
                set_tray_recording_status(
                    context.tray_recording_state,
                    context.tray_command_sender,
                    crate::tray::TrayRecordingStatus::Idle,
                );
            }
            terminal
        }
        Err(error) => {
            log_recording_runtime_failure(
                context.log,
                "progress-failed",
                &active.operation_id,
                error,
            );
            context
                .agent_events
                .desktop
                .recording_failed(&active.recording_id);
            publish_failure(
                context.agent_events,
                active.delivery_session.controller_lease_id.as_deref(),
                &active.operation_id,
                observed_epoch_ms,
                recording_error_kind(error),
            );
            set_tray_recording_status(
                context.tray_recording_state,
                context.tray_command_sender,
                crate::tray::TrayRecordingStatus::Idle,
            );
            true
        }
    }
}

fn publish_recording_events(
    events: Vec<crate::recording::RecordingCommandEvent>,
    controller_lease_id: Option<&str>,
    agent_events: &AgentEventHub,
    asset_sender: Option<&SyncSender<WorkerCommand>>,
) {
    for event in events {
        observe_desktop_recording(&agent_events.desktop, &event);
        if let crate::recording::RecordingCommandEvent::ManualFinalized(event) = &event {
            if enqueue_manual_recording_publish(asset_sender, event).is_err() {
                agent_events.desktop.recording_failed(&event.recording_id);
            }
        } else if let Some(controller_lease_id) = controller_lease_id
            && let Some(event) = command_events::recording_command_event(event, controller_lease_id)
        {
            let _ = agent_events.publish_command_event(event);
        }
    }
}

fn publish_start_failure(
    agent_events: &AgentEventHub,
    request: &RecordingStartRequest,
    kind: OperationalErrorKind,
) {
    publish_failure(
        agent_events,
        request.delivery_session.controller_lease_id.as_deref(),
        &request.operation_id,
        request.requested_at_epoch_ms,
        kind,
    );
}

fn publish_control_failure(
    agent_events: &AgentEventHub,
    request: &RecordingControlRequest,
    kind: OperationalErrorKind,
) {
    publish_failure(
        agent_events,
        request.delivery_session.controller_lease_id.as_deref(),
        &request.operation_id,
        request.requested_at_epoch_ms,
        kind,
    );
}

fn publish_failure(
    agent_events: &AgentEventHub,
    controller_lease_id: Option<&str>,
    command_id: &str,
    requested_at_epoch_ms: u64,
    kind: OperationalErrorKind,
) {
    let _ = agent_events.publish_command_failure(NativeHostCommandFailureEvent {
        controller_lease_id: controller_lease_id.map(str::to_owned),
        operation: OperationFailedOperation::Recording,
        operation_id: Some(command_id.to_owned()),
        phase: "recording",
        kind,
        occurred_at_epoch_ms: requested_at_epoch_ms,
    });
}

const fn recording_error_kind(error: RecordingRuntimeError) -> OperationalErrorKind {
    match error {
        RecordingRuntimeError::RuntimeUnavailable | RecordingRuntimeError::UnsupportedMode => {
            OperationalErrorKind::UnsupportedCapability
        }
        RecordingRuntimeError::DiskPressure => OperationalErrorKind::DiskPressure,
        RecordingRuntimeError::TransferState => OperationalErrorKind::StorageUnavailable,
        RecordingRuntimeError::InvalidState | RecordingRuntimeError::RuntimeFailed => {
            OperationalErrorKind::RecordingRuntimeFailed
        }
        #[cfg(windows)]
        RecordingRuntimeError::RuntimeFailure(_) => OperationalErrorKind::RecordingRuntimeFailed,
    }
}

fn observe_desktop_recording(
    projection: &super::super::desktop::projection::CaptureProjection,
    event: &crate::recording::RecordingCommandEvent,
) {
    use crate::recording::RecordingCommandEvent;
    use crate::tray::desktop::model::CapturePhase;
    match event {
        RecordingCommandEvent::Started(event) => {
            projection.recording(&event.recording_id, CapturePhase::Recording, 0);
        }
        RecordingCommandEvent::Progress(event) => {
            let phase = match event.status {
                crate::protocol::RecordingProgressStatus::Recording => CapturePhase::Recording,
                crate::protocol::RecordingProgressStatus::Paused => CapturePhase::Paused,
                crate::protocol::RecordingProgressStatus::Stopping
                | crate::protocol::RecordingProgressStatus::Finalizing => CapturePhase::Saving,
            };
            projection.recording(&event.recording_id, phase, event.duration_ms);
        }
        RecordingCommandEvent::ManualFinalized(event) => {
            projection.recording_finalized(&event.recording_id);
        }
        RecordingCommandEvent::Stopped(event) => {
            projection.recording_completed(&event.pending.recording_id);
        }
    }
}
