use std::{
    sync::{Arc, Mutex, mpsc},
    thread,
    time::Duration,
};

use super::*;
use crate::{
    app::resident::AgentOutboundEvent,
    telemetry::{
        RecordingTelemetryActionEvent, RecordingTelemetrySignal,
        SanitizedRecordingTelemetrySnapshot,
    },
};

#[test]
fn telemetry_sink_routes_lifecycle_commands_to_worker_source() -> Result<(), String> {
    let (command_sender, command_receiver) = mpsc::sync_channel(1);
    let (event_sender, event_receiver) = mpsc::sync_channel(1);
    let agent_events = AgentEventHub::default();
    let source = SharedTelemetrySource::new();
    let source_events = source.events.clone();
    let handle = thread::spawn(move || {
        run_telemetry_worker(
            AgentComponent::TelemetryWorker,
            &event_sender,
            &command_receiver,
            Box::new(source),
        );
    });
    assert_eq!(
        event_receiver.recv_timeout(Duration::from_millis(500)),
        Ok(WorkerEvent::Started(AgentComponent::TelemetryWorker))
    );

    let mut sink = TelemetryWorkerCommandSink::new(Some(command_sender.clone()), agent_events);
    sink.bind_controller_lease("lease:connection-1");
    sink.configure_recording(RecordingTelemetryPolicy::default());
    sink.start_recording("recording-1", 10);
    wait_for_events(&source_events, &["configure", "start:recording-1"])?;
    sink.pause_recording("recording-1", 20);
    wait_for_events(
        &source_events,
        &["configure", "start:recording-1", "pause:recording-1"],
    )?;
    sink.resume_recording("recording-1", 30);
    wait_for_events(
        &source_events,
        &[
            "configure",
            "start:recording-1",
            "pause:recording-1",
            "resume:recording-1",
        ],
    )?;
    let snapshot = sink
        .snapshot_for_recording("recording-1", 40)
        .ok_or_else(|| "snapshot reply missing".to_owned())?;

    assert_eq!(
        *source_events.lock().map_err(|_| "events lock".to_owned())?,
        vec![
            "configure",
            "start:recording-1",
            "pause:recording-1",
            "resume:recording-1",
            "snapshot:recording-1"
        ]
    );
    let protocol = snapshot.into_protocol();
    let serialized = serde_json::to_string(&protocol).map_err(|error| error.to_string())?;
    assert!(serialized.contains("Ctrl+S"));
    assert!(!serialized.contains("secret typed text"));
    command_sender
        .send(WorkerCommand::Stop)
        .map_err(|error| error.to_string())?;
    handle.join().map_err(|_| "join worker".to_owned())?;
    Ok(())
}

#[test]
fn telemetry_sink_reports_queue_full_without_raw_payload() -> Result<(), String> {
    let (command_sender, receiver) = mpsc::sync_channel(1);
    command_sender
        .send(WorkerCommand::Stop)
        .map_err(|error| error.to_string())?;
    let agent_events = AgentEventHub::default();
    let outbound = agent_events.subscribe();
    let mut sink = TelemetryWorkerCommandSink::new(Some(command_sender), agent_events);

    sink.bind_controller_lease("lease:connection-1");
    sink.pause_recording("recording-1", 10);

    assert!(matches!(
        outbound.recv_timeout(Duration::from_millis(500)),
        Ok(AgentOutboundEvent::CommandFailure(failure))
            if failure.controller_lease_id.as_deref() == Some("lease:connection-1")
                && failure.operation == OperationFailedOperation::Telemetry
                && failure.operation_id.as_deref() == Some("recording-1")
                && failure.kind == OperationalErrorKind::QueueBackpressure
    ));
    drop(receiver);
    Ok(())
}

#[test]
fn telemetry_start_waits_for_capacity_instead_of_dropping_command() -> Result<(), String> {
    let (command_sender, command_receiver) = mpsc::sync_channel(1);
    command_sender
        .send(WorkerCommand::PauseRecordingTelemetry {
            recording_id: "recording-previous".to_owned(),
            paused_at_monotonic_ns: 1,
        })
        .map_err(|error| error.to_string())?;
    let agent_events = AgentEventHub::default();
    let outbound = agent_events.subscribe();
    let mut sink = TelemetryWorkerCommandSink::new(Some(command_sender), agent_events);
    sink.bind_controller_lease("lease:connection-1");
    let (started_sender, started_receiver) = mpsc::sync_channel(1);
    let handle = thread::spawn(move || {
        let _ = started_sender.send(());
        sink.start_recording("recording-1", 10);
    });
    started_receiver
        .recv_timeout(Duration::from_millis(500))
        .map_err(|error| error.to_string())?;

    assert!(matches!(
        command_receiver.recv_timeout(Duration::from_millis(500)),
        Ok(WorkerCommand::PauseRecordingTelemetry { recording_id, .. })
            if recording_id == "recording-previous"
    ));
    handle.join().map_err(|_| "join start sender".to_owned())?;
    assert!(matches!(
        command_receiver.recv_timeout(Duration::from_millis(500)),
        Ok(WorkerCommand::StartRecordingTelemetry {
            recording_id,
            started_at_monotonic_ns,
            ..
        }) if recording_id == "recording-1" && started_at_monotonic_ns == 10
    ));
    assert!(outbound.try_recv().is_err());
    Ok(())
}

fn wait_for_events(
    events: &Arc<Mutex<Vec<&'static str>>>,
    expected: &[&'static str],
) -> Result<(), String> {
    let started = std::time::Instant::now();
    loop {
        let current = events.lock().map_err(|_| "events lock".to_owned())?.clone();
        if current.as_slice() == expected {
            return Ok(());
        }
        if started.elapsed() > Duration::from_millis(500) {
            return Err(format!("timed out waiting for events: {current:?}"));
        }
        thread::sleep(Duration::from_millis(5));
    }
}

#[derive(Clone, Debug)]
struct SharedTelemetrySource {
    events: Arc<Mutex<Vec<&'static str>>>,
}

impl SharedTelemetrySource {
    fn new() -> Self {
        Self {
            events: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn push(&self, event: &'static str) {
        if let Ok(mut events) = self.events.lock() {
            events.push(event);
        }
    }
}

impl RecordingTelemetrySource for SharedTelemetrySource {
    fn configure_recording(&mut self, _policy: RecordingTelemetryPolicy) {
        self.push("configure");
    }

    fn start_recording(&mut self, recording_id: &str, _started_at_monotonic_ns: u64) {
        assert_eq!(recording_id, "recording-1");
        self.push("start:recording-1");
    }

    fn pause_recording(&mut self, recording_id: &str, _paused_at_monotonic_ns: u64) {
        assert_eq!(recording_id, "recording-1");
        self.push("pause:recording-1");
    }

    fn resume_recording(&mut self, recording_id: &str, _resumed_at_monotonic_ns: u64) {
        assert_eq!(recording_id, "recording-1");
        self.push("resume:recording-1");
    }

    fn snapshot_for_recording(
        &mut self,
        recording_id: &str,
        _duration_ms: u64,
    ) -> Option<SanitizedRecordingTelemetrySnapshot> {
        assert_eq!(recording_id, "recording-1");
        self.push("snapshot:recording-1");
        Some(SanitizedRecordingTelemetrySnapshot::new(
            vec![RecordingTelemetryActionEvent::Key {
                label: "Ctrl+S".to_owned(),
                time_ms: 5,
            }],
            vec![RecordingTelemetrySignal::typing_span(3, 1, 4)],
        ))
    }
}
