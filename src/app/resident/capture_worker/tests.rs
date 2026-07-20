use std::{sync::mpsc, thread, time::Duration};

use super::*;
use crate::{
    app::resident::AgentOutboundEvent,
    capture::{
        ScreenshotCaptureCompletion, ScreenshotCaptureError, ScreenshotCaptureEvent,
        ScreenshotCaptureRequest,
    },
    protocol::{DeliveryRoute, OperationAuthority, ScreenshotMode},
};

#[test]
fn capture_worker_publishes_screenshot_start_event() -> Result<(), String> {
    let (command_sender, command_receiver) = mpsc::sync_channel(1);
    let (event_sender, event_receiver) = mpsc::sync_channel(1);
    let agent_events = AgentEventHub::default();
    let outbound = agent_events.subscribe();
    let worker_events = agent_events.clone();
    let handle = thread::spawn(move || {
        run_capture_worker(
            AgentComponent::CaptureWorker,
            &event_sender,
            &command_receiver,
            Some(Box::new(FakeScreenshotRunner::success())),
            &worker_events,
            None,
        );
    });
    assert_eq!(
        event_receiver.recv_timeout(Duration::from_millis(500)),
        Ok(WorkerEvent::Started(AgentComponent::CaptureWorker))
    );

    command_sender
        .send(WorkerCommand::CaptureScreenshot {
            request: Box::new(screenshot_request()),
            extension_controller_lease_id: Some("lease:connection-1".to_owned()),
        })
        .map_err(|error| error.to_string())?;

    assert!(matches!(
        outbound.recv_timeout(Duration::from_millis(500)),
        Ok(AgentOutboundEvent::CommandEvent(event))
            if matches!(
                event.as_ref(),
                crate::app::responses::NativeHostCommandEvent::ScreenshotStart(message)
                    if message.capture_id == "capture-1"
            )
    ));
    command_sender
        .send(WorkerCommand::Stop)
        .map_err(|error| error.to_string())?;
    handle.join().map_err(|_| "join worker".to_owned())?;
    Ok(())
}

#[test]
fn capture_worker_publishes_unsupported_mode_failure() -> Result<(), String> {
    let (command_sender, command_receiver) = mpsc::sync_channel(1);
    let (event_sender, event_receiver) = mpsc::sync_channel(1);
    let agent_events = AgentEventHub::default();
    let outbound = agent_events.subscribe();
    let worker_events = agent_events.clone();
    let handle = thread::spawn(move || {
        run_capture_worker(
            AgentComponent::CaptureWorker,
            &event_sender,
            &command_receiver,
            Some(Box::new(FakeScreenshotRunner::error(
                ScreenshotCaptureError::UnsupportedMode,
            ))),
            &worker_events,
            None,
        );
    });
    assert_eq!(
        event_receiver.recv_timeout(Duration::from_millis(500)),
        Ok(WorkerEvent::Started(AgentComponent::CaptureWorker))
    );

    command_sender
        .send(WorkerCommand::CaptureScreenshot {
            request: Box::new(screenshot_request()),
            extension_controller_lease_id: Some("lease:connection-1".to_owned()),
        })
        .map_err(|error| error.to_string())?;

    assert!(matches!(
        outbound.recv_timeout(Duration::from_millis(500)),
        Ok(AgentOutboundEvent::CommandFailure(failure))
            if failure.kind == OperationalErrorKind::UnsupportedCapability
                && failure.operation == OperationFailedOperation::Screenshot
                && failure.operation_id.as_deref() == Some("command-1")
    ));
    command_sender
        .send(WorkerCommand::Stop)
        .map_err(|error| error.to_string())?;
    handle.join().map_err(|_| "join worker".to_owned())?;
    Ok(())
}

#[test]
fn capture_sink_reports_queue_full_and_stopped_worker() -> Result<(), String> {
    let (sender, receiver) = mpsc::sync_channel(1);
    sender
        .send(WorkerCommand::Stop)
        .map_err(|error| error.to_string())?;
    let mut sink = CaptureWorkerCommandSink::new(Some(sender));
    assert_eq!(
        sink.start_screenshot_capture(screenshot_request(), Some("lease:connection-1")),
        Err(NativeHostSideEffectError::WorkerQueueFull)
    );
    drop(receiver);

    let (sender, receiver) = mpsc::sync_channel(1);
    drop(receiver);
    let mut sink = CaptureWorkerCommandSink::new(Some(sender));
    assert_eq!(
        sink.start_screenshot_capture(screenshot_request(), Some("lease:connection-1")),
        Err(NativeHostSideEffectError::WorkerStopped)
    );
    Ok(())
}

#[test]
fn capture_worker_queue_backpressure_burst_stays_bounded() -> Result<(), String> {
    let (sender, receiver) = mpsc::sync_channel(1);
    sender
        .send(WorkerCommand::Stop)
        .map_err(|error| error.to_string())?;
    let mut sink = CaptureWorkerCommandSink::new(Some(sender));

    for _ in 0..16 {
        assert_eq!(
            sink.start_screenshot_capture(screenshot_request(), Some("lease:connection-1")),
            Err(NativeHostSideEffectError::WorkerQueueFull)
        );
    }

    assert!(matches!(receiver.try_recv(), Ok(WorkerCommand::Stop)));
    assert!(receiver.try_recv().is_err());
    Ok(())
}

#[derive(Debug)]
struct FakeScreenshotRunner {
    result: Result<Vec<ScreenshotCaptureEvent>, ScreenshotCaptureError>,
}

impl FakeScreenshotRunner {
    fn success() -> Self {
        Self {
            result: Ok(vec![ScreenshotCaptureEvent::Completed(
                ScreenshotCaptureCompletion {
                    authority: OperationAuthority::ExtensionManaged {
                        settings_revision: "settings-revision-1".to_owned(),
                    },
                    delivery_route: DeliveryRoute::ExtensionTransfer,
                    capture_id: "capture-1".to_owned(),
                    mode: ScreenshotMode::Screen,
                    filename: "capture-1.png".to_owned(),
                    total_bytes: 11,
                    width: 10,
                    height: 10,
                    chunk_count: 1,
                    sha256: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                        .to_owned(),
                    open_editor: true,
                    captured_at_epoch_ms: 4_000,
                },
            )]),
        }
    }

    fn error(error: ScreenshotCaptureError) -> Self {
        Self { result: Err(error) }
    }
}

impl ScreenshotCaptureRunner for FakeScreenshotRunner {
    fn capture_screenshot(
        &mut self,
        _request: &ScreenshotCaptureRequest,
    ) -> Result<Vec<ScreenshotCaptureEvent>, ScreenshotCaptureError> {
        self.result.clone()
    }
}

fn screenshot_request() -> ScreenshotCaptureRequest {
    ScreenshotCaptureRequest {
        operation_id: "command-1".to_owned(),
        mode: ScreenshotMode::Screen,
        authority: OperationAuthority::ExtensionManaged {
            settings_revision: "settings-revision-1".to_owned(),
        },
        delivery_route: DeliveryRoute::ExtensionTransfer,
        include_cursor: true,
        open_editor: true,
        requested_at_epoch_ms: 4_000,
    }
}
