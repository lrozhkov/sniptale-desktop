//! Resident screenshot worker command broker.

use std::sync::mpsc::{Receiver, SyncSender, TrySendError};

use crate::{
    app::responses::{
        NativeHostCommandFailureEvent, NativeHostCommandOutcome, NativeHostSideEffectError,
        screenshot_command_event,
    },
    capture::{
        ScreenshotCaptureError, ScreenshotCaptureEvent, ScreenshotCaptureRequest,
        ScreenshotCaptureRunner,
    },
    protocol::{DeliveryRoute, OperationAuthority, OperationFailedOperation, OperationalErrorKind},
};

use super::{AgentComponent, AgentEventHub, WorkerCommand, WorkerEvent};

#[derive(Clone, Debug)]
pub(super) struct CaptureWorkerCommandSink {
    sender: Option<SyncSender<WorkerCommand>>,
}

impl CaptureWorkerCommandSink {
    pub(super) fn new(sender: Option<SyncSender<WorkerCommand>>) -> Self {
        Self { sender }
    }

    pub(super) fn start_screenshot_capture(
        &mut self,
        request: ScreenshotCaptureRequest,
        extension_controller_lease_id: Option<&str>,
    ) -> Result<NativeHostCommandOutcome, NativeHostSideEffectError> {
        let sender = self
            .sender
            .as_ref()
            .ok_or(NativeHostSideEffectError::UnsupportedCapability)?;
        match sender.try_send(WorkerCommand::CaptureScreenshot {
            extension_controller_lease_id: extension_controller_lease_id.map(str::to_owned),
            request: Box::new(request),
        }) {
            Ok(()) => Ok(NativeHostCommandOutcome::Accepted),
            Err(TrySendError::Full(_)) => Err(NativeHostSideEffectError::WorkerQueueFull),
            Err(TrySendError::Disconnected(_)) => Err(NativeHostSideEffectError::WorkerStopped),
        }
    }
}

pub(super) fn run_capture_worker(
    component: AgentComponent,
    event_sender: &SyncSender<WorkerEvent>,
    command_receiver: &Receiver<WorkerCommand>,
    mut runner: Option<Box<dyn ScreenshotCaptureRunner>>,
    agent_events: &AgentEventHub,
    asset_sender: Option<&SyncSender<WorkerCommand>>,
) {
    let _ = event_sender.send(WorkerEvent::Started(component));
    let mut accepted_since_fence = false;
    loop {
        match command_receiver.recv() {
            Ok(WorkerCommand::Stop) | Err(_) => break,
            Ok(WorkerCommand::FenceMode { result_sender }) => {
                let _ = result_sender.send(accepted_since_fence);
                accepted_since_fence = false;
            }
            Ok(WorkerCommand::CaptureScreenshot {
                request,
                extension_controller_lease_id,
            }) => {
                accepted_since_fence = true;
                run_capture_command(
                    &request,
                    extension_controller_lease_id.as_deref(),
                    &mut runner,
                    agent_events,
                    asset_sender,
                );
            }
            Ok(
                WorkerCommand::ApplyHotkeyPlan { .. }
                | WorkerCommand::ManualHotkeyAction { .. }
                | WorkerCommand::RestoreHotkeys { .. }
                | WorkerCommand::ApplyTrayRegistry { .. }
                | WorkerCommand::ShowTrayNotice { .. }
                | WorkerCommand::UpdateTrayRecordingStatus { .. }
                | WorkerCommand::RefreshTrayConnectionStatus
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

fn run_capture_command(
    request: &ScreenshotCaptureRequest,
    extension_controller_lease_id: Option<&str>,
    runner: &mut Option<Box<dyn ScreenshotCaptureRunner>>,
    agent_events: &AgentEventHub,
    asset_sender: Option<&SyncSender<WorkerCommand>>,
) {
    let Some(runner) = runner.as_mut() else {
        publish_capture_failure(
            agent_events,
            request,
            extension_controller_lease_id,
            OperationalErrorKind::UnsupportedCapability,
        );
        return;
    };
    match runner.capture_screenshot(request) {
        Ok(events) => {
            for event in events {
                if let ScreenshotCaptureEvent::Completed(completion) = &event
                    && let DeliveryRoute::LocalAsset { asset_job_id } = &completion.delivery_route
                {
                    enqueue_manual_asset_publish(asset_sender, completion, asset_job_id);
                }
                if let Some(event) = screenshot_command_event(event, extension_controller_lease_id)
                {
                    let _ = agent_events.publish_command_event(event);
                }
            }
        }
        Err(error) => publish_capture_failure(
            agent_events,
            request,
            extension_controller_lease_id,
            capture_error_kind(error),
        ),
    }
}

fn enqueue_manual_asset_publish(
    asset_sender: Option<&SyncSender<WorkerCommand>>,
    completion: &crate::capture::ScreenshotCaptureCompletion,
    asset_job_id: &str,
) {
    let Some(sender) = asset_sender else {
        return;
    };
    let Ok(job_id) = crate::assets::AssetJobId::parse(asset_job_id.to_owned()) else {
        return;
    };
    let Ok(store) = crate::capture::ScreenshotTransferStore::open_default() else {
        return;
    };
    let Ok(reader) = store.manual_asset_reader(&completion.capture_id, asset_job_id) else {
        return;
    };
    let _ = sender.try_send(WorkerCommand::PublishManualAsset {
        job_id,
        source: Box::new(reader),
    });
}

fn publish_capture_failure(
    agent_events: &AgentEventHub,
    request: &ScreenshotCaptureRequest,
    extension_controller_lease_id: Option<&str>,
    kind: OperationalErrorKind,
) {
    if matches!(
        (&request.authority, &request.delivery_route),
        (
            OperationAuthority::ExtensionManaged { .. },
            DeliveryRoute::ExtensionTransfer
        )
    ) {
        let _ = agent_events.publish_command_failure(NativeHostCommandFailureEvent {
            controller_lease_id: extension_controller_lease_id.map(str::to_owned),
            operation: OperationFailedOperation::Screenshot,
            operation_id: Some(request.operation_id.clone()),
            phase: "screenshot",
            kind,
            occurred_at_epoch_ms: request.requested_at_epoch_ms,
        });
    }
}

fn capture_error_kind(error: ScreenshotCaptureError) -> OperationalErrorKind {
    match error {
        ScreenshotCaptureError::UnsupportedMode
        | ScreenshotCaptureError::UnsupportedDeliveryRoute => {
            OperationalErrorKind::UnsupportedCapability
        }
        #[cfg(windows)]
        ScreenshotCaptureError::RegionSelectionCancelled => {
            OperationalErrorKind::ScreenshotSelectionCancelled
        }
        #[cfg(windows)]
        ScreenshotCaptureError::InvalidSelection => {
            OperationalErrorKind::ScreenshotInvalidSelection
        }
        ScreenshotCaptureError::TransferState => OperationalErrorKind::StorageUnavailable,
        ScreenshotCaptureError::CaptureUnavailable
        | ScreenshotCaptureError::RawFrameTooLarge
        | ScreenshotCaptureError::InvalidFrame
        | ScreenshotCaptureError::FileIo => OperationalErrorKind::ScreenshotCaptureFailed,
        #[cfg(windows)]
        ScreenshotCaptureError::PngEncodeFailed => OperationalErrorKind::ScreenshotCaptureFailed,
    }
}

#[cfg(test)]
mod tests;
