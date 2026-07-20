use std::{sync::mpsc, thread, time::Duration};

use super::status::set_tray_recording_status;
use super::*;
use crate::{
    app::resident::{AgentOutboundEvent, tray::TrayRecordingState},
    protocol::{
        ExtensionRecordingControlCommandMessage, ExtensionRecordingStartCommandMessage,
        NativeAudioSourceMode, NativeRecordingSource, NativeRequestedQualitySettings,
        NativeVideoFrameRate, NativeVideoQuality, OperationFailedOperation, OperationalErrorKind,
        PROTOCOL_VERSION, RecordingControl, RecordingProgressStatus, VideoMode,
    },
    recording::{
        PendingRecordingTransfer, RecordingCommandEvent, RecordingProgressEvent,
        RecordingRuntimeError, RecordingStoppedEvent,
    },
};

#[test]
fn recording_worker_publishes_start_progress_event() -> Result<(), String> {
    let (command_sender, command_receiver) = mpsc::sync_channel(1);
    let (event_sender, event_receiver) = mpsc::sync_channel(1);
    let agent_events = AgentEventHub::default();
    let outbound = agent_events.subscribe();
    let worker_events = agent_events.clone();
    let handle = thread::spawn(move || {
        run_recording_worker(
            AgentComponent::EncoderWorker,
            RecordingWorkerInputs {
                event_sender: &event_sender,
                command_receiver: &command_receiver,
                runner: Some(Box::new(FakeRecordingRunner::success())),
                agent_events: &worker_events,
                tray_command_sender: None,
                tray_recording_state: &TrayRecordingState::default(),
                asset_sender: None,
                log: None,
            },
        );
    });
    assert_eq!(
        event_receiver.recv_timeout(Duration::from_millis(500)),
        Ok(WorkerEvent::Started(AgentComponent::EncoderWorker))
    );

    command_sender
        .send(WorkerCommand::StartRecording {
            request: Box::new(recording_start_request()),
            policy: RecordingStartPolicy::default(),
        })
        .map_err(|error| error.to_string())?;

    assert!(matches!(
        outbound.recv_timeout(Duration::from_millis(500)),
        Ok(AgentOutboundEvent::CommandEvent(event))
            if matches!(
                event.as_ref(),
                crate::app::responses::NativeHostCommandEvent::RecordingProgress(message)
                    if message.recording_id == "recording-1"
            )
    ));
    command_sender
        .send(WorkerCommand::Stop)
        .map_err(|error| error.to_string())?;
    handle.join().map_err(|_| "join worker".to_owned())?;
    Ok(())
}

#[test]
fn recording_worker_publishes_control_stop_event() -> Result<(), String> {
    let (command_sender, command_receiver) = mpsc::sync_channel(1);
    let (event_sender, event_receiver) = mpsc::sync_channel(1);
    let agent_events = AgentEventHub::default();
    let outbound = agent_events.subscribe();
    let worker_events = agent_events.clone();
    let handle = thread::spawn(move || {
        run_recording_worker(
            AgentComponent::EncoderWorker,
            RecordingWorkerInputs {
                event_sender: &event_sender,
                command_receiver: &command_receiver,
                runner: Some(Box::new(FakeRecordingRunner::success())),
                agent_events: &worker_events,
                tray_command_sender: None,
                tray_recording_state: &TrayRecordingState::default(),
                asset_sender: None,
                log: None,
            },
        );
    });
    assert_eq!(
        event_receiver.recv_timeout(Duration::from_millis(500)),
        Ok(WorkerEvent::Started(AgentComponent::EncoderWorker))
    );

    command_sender
        .send(WorkerCommand::ControlRecording {
            request: Box::new(recording_control_request()),
        })
        .map_err(|error| error.to_string())?;

    assert!(matches!(
        outbound.recv_timeout(Duration::from_millis(500)),
        Ok(AgentOutboundEvent::CommandEvent(event))
            if matches!(
                event.as_ref(),
                crate::app::responses::NativeHostCommandEvent::RecordingStopped(message)
                    if message.recording_id == "recording-1"
            )
    ));
    command_sender
        .send(WorkerCommand::Stop)
        .map_err(|error| error.to_string())?;
    handle.join().map_err(|_| "join worker".to_owned())?;
    Ok(())
}

#[test]
fn recording_worker_accepts_local_safety_stop_after_extension_start() -> Result<(), String> {
    let (command_sender, command_receiver) = mpsc::sync_channel(2);
    let (event_sender, event_receiver) = mpsc::sync_channel(1);
    let agent_events = AgentEventHub::default();
    let outbound = agent_events.subscribe();
    let worker_events = agent_events.clone();
    let handle = thread::spawn(move || {
        run_recording_worker(
            AgentComponent::EncoderWorker,
            RecordingWorkerInputs {
                event_sender: &event_sender,
                command_receiver: &command_receiver,
                runner: Some(Box::new(FakeRecordingRunner::success())),
                agent_events: &worker_events,
                tray_command_sender: None,
                tray_recording_state: &TrayRecordingState::default(),
                asset_sender: None,
                log: None,
            },
        );
    });
    assert_eq!(
        event_receiver.recv_timeout(Duration::from_millis(500)),
        Ok(WorkerEvent::Started(AgentComponent::EncoderWorker))
    );

    command_sender
        .send(WorkerCommand::StartRecording {
            request: Box::new(recording_start_request()),
            policy: RecordingStartPolicy::default(),
        })
        .map_err(|error| error.to_string())?;
    let _ = outbound.recv_timeout(Duration::from_millis(500));
    command_sender
        .send(WorkerCommand::ControlActiveRecording {
            control: RecordingControl::Stop,
        })
        .map_err(|error| error.to_string())?;

    assert!(matches!(
        outbound.recv_timeout(Duration::from_millis(500)),
        Ok(AgentOutboundEvent::CommandEvent(event))
            if matches!(
                event.as_ref(),
                crate::app::responses::NativeHostCommandEvent::RecordingStopped(message)
                    if message.recording_id == "recording-1"
            )
    ));
    command_sender
        .send(WorkerCommand::Stop)
        .map_err(|error| error.to_string())?;
    handle.join().map_err(|_| "join worker".to_owned())?;
    Ok(())
}

#[test]
fn rejected_second_start_keeps_local_safety_control_for_the_first_recording() -> Result<(), String>
{
    let (command_sender, command_receiver) = mpsc::sync_channel(3);
    let (event_sender, event_receiver) = mpsc::sync_channel(1);
    let agent_events = AgentEventHub::default();
    let outbound = agent_events.subscribe();
    let worker_events = agent_events.clone();
    let handle = thread::spawn(move || {
        run_recording_worker(
            AgentComponent::EncoderWorker,
            RecordingWorkerInputs {
                event_sender: &event_sender,
                command_receiver: &command_receiver,
                runner: Some(Box::new(FakeRecordingRunner::success())),
                agent_events: &worker_events,
                tray_command_sender: None,
                tray_recording_state: &TrayRecordingState::default(),
                asset_sender: None,
                log: None,
            },
        );
    });
    assert_eq!(
        event_receiver.recv_timeout(Duration::from_millis(500)),
        Ok(WorkerEvent::Started(AgentComponent::EncoderWorker))
    );

    command_sender
        .send(WorkerCommand::StartRecording {
            request: Box::new(recording_start_request()),
            policy: RecordingStartPolicy::default(),
        })
        .map_err(|error| error.to_string())?;
    let _ = outbound.recv_timeout(Duration::from_millis(500));
    let mut second = recording_start_request();
    second.operation_id = "command-2".to_owned();
    command_sender
        .send(WorkerCommand::StartRecording {
            request: Box::new(second),
            policy: RecordingStartPolicy::default(),
        })
        .map_err(|error| error.to_string())?;
    assert!(matches!(
        outbound.recv_timeout(Duration::from_millis(500)),
        Ok(AgentOutboundEvent::CommandFailure(failure))
            if failure.operation_id.as_deref() == Some("command-2")
                && failure.kind == OperationalErrorKind::RecordingRuntimeFailed
    ));
    command_sender
        .send(WorkerCommand::ControlActiveRecording {
            control: RecordingControl::Stop,
        })
        .map_err(|error| error.to_string())?;
    assert!(matches!(
        outbound.recv_timeout(Duration::from_millis(500)),
        Ok(AgentOutboundEvent::CommandEvent(event))
            if matches!(
                event.as_ref(),
                crate::app::responses::NativeHostCommandEvent::RecordingStopped(message)
                    if message.recording_id == "recording-1"
            )
    ));
    command_sender
        .send(WorkerCommand::Stop)
        .map_err(|error| error.to_string())?;
    handle.join().map_err(|_| "join worker".to_owned())?;
    Ok(())
}

#[test]
fn recording_worker_publishes_disk_pressure_failure() -> Result<(), String> {
    let (command_sender, command_receiver) = mpsc::sync_channel(1);
    let (event_sender, event_receiver) = mpsc::sync_channel(1);
    let agent_events = AgentEventHub::default();
    let outbound = agent_events.subscribe();
    let worker_events = agent_events.clone();
    let handle = thread::spawn(move || {
        run_recording_worker(
            AgentComponent::EncoderWorker,
            RecordingWorkerInputs {
                event_sender: &event_sender,
                command_receiver: &command_receiver,
                runner: Some(Box::new(FakeRecordingRunner::error(
                    RecordingRuntimeError::DiskPressure,
                ))),
                agent_events: &worker_events,
                tray_command_sender: None,
                tray_recording_state: &TrayRecordingState::default(),
                asset_sender: None,
                log: None,
            },
        );
    });
    assert_eq!(
        event_receiver.recv_timeout(Duration::from_millis(500)),
        Ok(WorkerEvent::Started(AgentComponent::EncoderWorker))
    );

    command_sender
        .send(WorkerCommand::StartRecording {
            request: Box::new(recording_start_request()),
            policy: RecordingStartPolicy::default(),
        })
        .map_err(|error| error.to_string())?;

    assert!(matches!(
        outbound.recv_timeout(Duration::from_millis(500)),
        Ok(AgentOutboundEvent::CommandFailure(failure))
            if failure.kind == OperationalErrorKind::DiskPressure
                && failure.operation == OperationFailedOperation::Recording
                && failure.operation_id.as_deref() == Some("command-1")
    ));
    command_sender
        .send(WorkerCommand::Stop)
        .map_err(|error| error.to_string())?;
    handle.join().map_err(|_| "join worker".to_owned())?;
    Ok(())
}

#[test]
fn manual_recording_start_failure_returns_idle_when_tray_queue_is_full() -> Result<(), String> {
    let (command_sender, command_receiver) = mpsc::sync_channel(1);
    let (event_sender, event_receiver) = mpsc::sync_channel(1);
    let (tray_sender, tray_receiver) = mpsc::sync_channel(1);
    let agent_events = AgentEventHub::default();
    let worker_events = agent_events.clone();
    let tray_recording_state = TrayRecordingState::default();
    let worker_tray_recording_state = tray_recording_state.clone();
    let handle = thread::spawn(move || {
        run_recording_worker(
            AgentComponent::EncoderWorker,
            RecordingWorkerInputs {
                event_sender: &event_sender,
                command_receiver: &command_receiver,
                runner: Some(Box::new(FakeRecordingRunner::error(
                    RecordingRuntimeError::RuntimeFailed,
                ))),
                agent_events: &worker_events,
                tray_command_sender: Some(&tray_sender),
                tray_recording_state: &worker_tray_recording_state,
                asset_sender: None,
                log: None,
            },
        );
    });
    assert_eq!(
        event_receiver.recv_timeout(Duration::from_millis(500)),
        Ok(WorkerEvent::Started(AgentComponent::EncoderWorker))
    );

    let mut request = recording_start_request();
    request.authority = crate::protocol::OperationAuthority::Manual {
        profile_revision: "8".to_owned(),
    };
    request.delivery_route = crate::protocol::DeliveryRoute::LocalAsset {
        asset_job_id: "asset-manual-recording".to_owned(),
    };
    request.delivery_session = crate::recording::RecordingDeliverySession::default();
    command_sender
        .send(WorkerCommand::StartRecording {
            request: Box::new(request),
            policy: RecordingStartPolicy::default(),
        })
        .map_err(|error| error.to_string())?;

    assert!(matches!(
        tray_receiver.recv_timeout(Duration::from_millis(500)),
        Ok(WorkerCommand::UpdateTrayRecordingStatus {
            status: crate::tray::TrayRecordingStatus::Preparing
        })
    ));
    command_sender
        .send(WorkerCommand::Stop)
        .map_err(|error| error.to_string())?;
    handle.join().map_err(|_| "join worker".to_owned())?;
    assert_eq!(
        tray_recording_state.status(),
        crate::tray::TrayRecordingStatus::Idle
    );
    Ok(())
}

#[test]
fn recording_worker_reports_runtime_unavailable_as_unsupported() -> Result<(), String> {
    let (command_sender, command_receiver) = mpsc::sync_channel(1);
    let (event_sender, event_receiver) = mpsc::sync_channel(1);
    let agent_events = AgentEventHub::default();
    let outbound = agent_events.subscribe();
    let worker_events = agent_events.clone();
    let handle = thread::spawn(move || {
        run_recording_worker(
            AgentComponent::EncoderWorker,
            RecordingWorkerInputs {
                event_sender: &event_sender,
                command_receiver: &command_receiver,
                runner: Some(Box::new(FakeRecordingRunner::error(
                    RecordingRuntimeError::RuntimeUnavailable,
                ))),
                agent_events: &worker_events,
                tray_command_sender: None,
                tray_recording_state: &TrayRecordingState::default(),
                asset_sender: None,
                log: None,
            },
        );
    });
    assert_eq!(
        event_receiver.recv_timeout(Duration::from_millis(500)),
        Ok(WorkerEvent::Started(AgentComponent::EncoderWorker))
    );

    command_sender
        .send(WorkerCommand::ControlRecording {
            request: Box::new(recording_control_request()),
        })
        .map_err(|error| error.to_string())?;

    assert!(matches!(
        outbound.recv_timeout(Duration::from_millis(500)),
        Ok(AgentOutboundEvent::CommandFailure(failure))
            if failure.kind == OperationalErrorKind::UnsupportedCapability
    ));
    command_sender
        .send(WorkerCommand::Stop)
        .map_err(|error| error.to_string())?;
    handle.join().map_err(|_| "join worker".to_owned())?;
    Ok(())
}

#[test]
fn recording_worker_queue_backpressure_burst_stays_bounded() -> Result<(), String> {
    let (sender, receiver) = mpsc::sync_channel(1);
    sender
        .send(WorkerCommand::Stop)
        .map_err(|error| error.to_string())?;
    let mut sink = RecordingWorkerCommandSink::new(Some(sender));

    for _ in 0..16 {
        assert_eq!(
            sink.start_recording(&recording_start_command(), RecordingStartPolicy::default()),
            Err(NativeHostSideEffectError::WorkerQueueFull)
        );
        assert_eq!(
            sink.control_recording(&recording_control_command()),
            Err(NativeHostSideEffectError::WorkerQueueFull)
        );
    }

    assert!(matches!(receiver.try_recv(), Ok(WorkerCommand::Stop)));
    assert!(receiver.try_recv().is_err());
    Ok(())
}

#[test]
fn recording_worker_forwards_lifecycle_status_to_tray() {
    let (sender, receiver) = mpsc::sync_channel(1);
    let state = TrayRecordingState::default();

    set_tray_recording_status(
        &state,
        Some(&sender),
        crate::tray::TrayRecordingStatus::Paused,
    );

    assert!(matches!(
        receiver.try_recv(),
        Ok(WorkerCommand::UpdateTrayRecordingStatus {
            status: crate::tray::TrayRecordingStatus::Paused
        })
    ));
    assert_eq!(state.status(), crate::tray::TrayRecordingStatus::Paused);
}

#[test]
fn recording_worker_coalesces_terminal_idle_when_tray_queue_is_full() -> Result<(), String> {
    let (sender, receiver) = mpsc::sync_channel(1);
    let state = TrayRecordingState::default();
    sender
        .send(WorkerCommand::Stop)
        .map_err(|error| error.to_string())?;

    set_tray_recording_status(
        &state,
        Some(&sender),
        crate::tray::TrayRecordingStatus::Finalizing,
    );
    set_tray_recording_status(
        &state,
        Some(&sender),
        crate::tray::TrayRecordingStatus::Idle,
    );

    assert_eq!(state.status(), crate::tray::TrayRecordingStatus::Idle);
    assert!(matches!(receiver.try_recv(), Ok(WorkerCommand::Stop)));
    assert!(receiver.try_recv().is_err());
    Ok(())
}

#[derive(Debug)]
struct FakeRecordingRunner {
    result: Result<(), RecordingRuntimeError>,
}

impl FakeRecordingRunner {
    fn success() -> Self {
        Self { result: Ok(()) }
    }

    fn error(error: RecordingRuntimeError) -> Self {
        Self { result: Err(error) }
    }
}

impl RecordingRunner for FakeRecordingRunner {
    fn start_recording(
        &mut self,
        _request: &crate::recording::RecordingStartRequest,
        policy: RecordingStartPolicy,
    ) -> Result<Vec<RecordingCommandEvent>, RecordingRuntimeError> {
        self.result?;
        assert_eq!(policy, RecordingStartPolicy::default());
        Ok(vec![RecordingCommandEvent::Progress(
            RecordingProgressEvent {
                recording_id: "recording-1".to_owned(),
                duration_ms: 0,
                bytes_written: 0,
                status: RecordingProgressStatus::Recording,
            },
        )])
    }

    fn control_recording(
        &mut self,
        _request: &crate::recording::RecordingControlRequest,
    ) -> Result<Vec<RecordingCommandEvent>, RecordingRuntimeError> {
        self.result?;
        Ok(vec![RecordingCommandEvent::Stopped(
            RecordingStoppedEvent {
                pending: PendingRecordingTransfer {
                    recording_id: "recording-1".to_owned(),
                    filename: "recording-1.mp4".to_owned(),
                    duration_ms: 1_000,
                    total_bytes: 11,
                    width: 10,
                    height: 10,
                    fps: 30,
                    chunk_count: 1,
                    next_missing_chunk_index: 0,
                    sha256: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                        .to_owned(),
                    open_editor: true,
                },
                telemetry: None,
            },
        )])
    }
}

fn recording_start_command() -> ExtensionRecordingStartCommandMessage {
    ExtensionRecordingStartCommandMessage {
        protocol_version: PROTOCOL_VERSION,
        controller_lease_id: "lease:connection-1".to_owned(),
        command_id: "command-1".to_owned(),
        source: NativeRecordingSource {
            mode: VideoMode::Screen,
            display_id: None,
            window_id: None,
            region: None,
            microphone_device_id: None,
        },
        settings_revision: "settings-revision-1".to_owned(),
        requested_quality: NativeRequestedQualitySettings {
            audio_bitrate_kbps: 160,
            audio_source_mode: NativeAudioSourceMode::Mixed,
            frame_rate: NativeVideoFrameRate::Auto,
            quality: NativeVideoQuality::High,
            video_bitrate_mbps_override: None,
        },
        open_editor: true,
        requested_at_epoch_ms: 4_000,
    }
}

fn recording_control_command() -> ExtensionRecordingControlCommandMessage {
    ExtensionRecordingControlCommandMessage {
        protocol_version: PROTOCOL_VERSION,
        controller_lease_id: "lease:connection-1".to_owned(),
        command_id: "command-1".to_owned(),
        recording_id: "recording-1".to_owned(),
        control: RecordingControl::Stop,
        requested_at_epoch_ms: 4_000,
    }
}

fn recording_start_request() -> crate::recording::RecordingStartRequest {
    extension_start_request(&recording_start_command())
}

fn recording_control_request() -> crate::recording::RecordingControlRequest {
    extension_control_request(&recording_control_command())
}
