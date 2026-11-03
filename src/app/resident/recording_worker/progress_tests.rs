use super::*;
use crate::tray::desktop::model::{
    Availability, CaptureCapabilities, CaptureKind, CapturePhase, CaptureQuality, CaptureTargets,
    Permission,
};

#[derive(Clone, Copy)]
enum TickOutcome {
    Progress,
    Stopped,
    PublicationFailure,
    Failure,
}
struct TickRunner {
    outcome: TickOutcome,
    recording: String,
}
impl RecordingRunner for TickRunner {
    fn tick(
        &mut self,
        _observed_epoch_ms: u64,
    ) -> Result<Vec<RecordingCommandEvent>, RecordingRuntimeError> {
        match self.outcome {
            TickOutcome::Progress => Ok(vec![RecordingCommandEvent::Progress(
                RecordingProgressEvent {
                    recording_id: self.recording.clone(),
                    duration_ms: 1000,
                    bytes_written: 0,
                    status: RecordingProgressStatus::Recording,
                },
            )]),
            TickOutcome::Stopped => {
                let mut events = FakeRecordingRunner::success()
                    .control_recording(&recording_control_request())?;
                if let RecordingCommandEvent::Stopped(event) = &mut events[0] {
                    event.pending.recording_id.clone_from(&self.recording);
                }
                Ok(events)
            }
            TickOutcome::PublicationFailure => Ok(vec![RecordingCommandEvent::ManualFinalized(
                crate::recording::ManualRecordingFinalizedEvent {
                    recording_id: self.recording.clone(),
                    asset_job_id: "fixture-job".to_owned(),
                    delivery_route: crate::protocol::DeliveryRoute::LocalAsset {
                        asset_job_id: "fixture-job".to_owned(),
                    },
                },
            )]),
            TickOutcome::Failure => Err(RecordingRuntimeError::RuntimeFailed),
        }
    }
    fn start_recording(
        &mut self,
        request: &RecordingStartRequest,
        policy: RecordingStartPolicy,
    ) -> Result<Vec<RecordingCommandEvent>, RecordingRuntimeError> {
        self.recording =
            crate::transfer::identity::derived_media_id("recording", &request.operation_id);
        assert_eq!(policy, RecordingStartPolicy::default());
        Ok(vec![RecordingCommandEvent::Progress(
            RecordingProgressEvent {
                recording_id: self.recording.clone(),
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
        Ok(Vec::new())
    }
}

struct TickFixture {
    command_sender: mpsc::SyncSender<WorkerCommand>,
    commands: mpsc::Receiver<WorkerCommand>,
    event_sender: mpsc::SyncSender<WorkerEvent>,
    _events: mpsc::Receiver<WorkerEvent>,
    events: AgentEventHub,
    tray: TrayRecordingState,
}
impl TickFixture {
    fn new() -> Self {
        let (command_sender, commands) = mpsc::sync_channel(1);
        let (event_sender, events) = mpsc::sync_channel(1);
        Self {
            command_sender,
            commands,
            event_sender,
            _events: events,
            events: AgentEventHub::default(),
            tray: TrayRecordingState::default(),
        }
    }
    fn inputs(&self, outcome: TickOutcome) -> RecordingWorkerInputs<'_> {
        RecordingWorkerInputs {
            event_sender: &self.event_sender,
            command_receiver: &self.commands,
            runner: Some(Box::new(TickRunner {
                outcome,
                recording: String::new(),
            })),
            agent_events: &self.events,
            tray_command_sender: None,
            tray_recording_state: &self.tray,
            asset_sender: None,
            log: None,
        }
    }
    fn begin(&self) {
        let request = recording_start_request();
        let recording =
            crate::transfer::identity::derived_media_id("recording", &request.operation_id);
        self.events.desktop.begin(
            &request.operation_id,
            None,
            CaptureKind::Video,
            CapturePhase::Starting,
            Some(&recording),
        );
    }
    fn elapsed(&self) -> Result<u64, String> {
        let available = Availability {
            supported: true,
            source_available: true,
            permission: Permission::NotRequired,
        };
        self.events
            .desktop
            .snapshot(
                CaptureCapabilities {
                    screenshot: available,
                    video: available,
                    targets: CaptureTargets {
                        region: available,
                        display: available,
                        window: available,
                    },
                },
                CaptureQuality::High,
            )
            .map(|snapshot| snapshot.elapsed_ms)
            .map_err(|_| "projection unavailable".to_owned())
    }
}

#[test]
fn owner_ticks_update_projection_and_terminal_ticks_release_worker_identity() -> Result<(), String>
{
    for outcome in [
        TickOutcome::Progress,
        TickOutcome::Stopped,
        TickOutcome::PublicationFailure,
        TickOutcome::Failure,
    ] {
        let fixture = TickFixture::new();
        fixture.begin();
        let mut runtime = RecordingWorkerRuntime::new(fixture.inputs(outcome));
        runtime.start(&recording_start_request(), RecordingStartPolicy::default());
        runtime.tick(5000);
        if matches!(outcome, TickOutcome::Progress) {
            assert_eq!(fixture.elapsed()?, 1000);
            assert!(runtime.active_recording.is_some());
        } else {
            assert!(runtime.active_recording.is_none());
            assert_eq!(
                fixture.tray.status(),
                crate::tray::TrayRecordingStatus::Idle
            );
            if matches!(
                outcome,
                TickOutcome::PublicationFailure | TickOutcome::Failure
            ) {
                assert_eq!(fixture.events.desktop.phase(), Ok(CapturePhase::Failed));
            }
            let mut next = recording_start_request();
            next.operation_id = "next-recording".to_owned();
            runtime.start(&next, RecordingStartPolicy::default());
            assert!(
                runtime.active_recording.is_some(),
                "A terminal tick must allow the next recording"
            );
        }
    }
    Ok(())
}

#[test]
fn active_worker_emits_progress_without_commands_and_under_a_busy_queue() -> Result<(), String> {
    for busy in [false, true] {
        run_cadence_case(busy)?;
    }
    Ok(())
}

fn run_cadence_case(busy: bool) -> Result<(), String> {
    let fixture = TickFixture::new();
    fixture.begin();
    let commands = fixture.command_sender.clone();
    let outbound = fixture.events.subscribe();
    let worker = thread::spawn(move || {
        run_recording_worker(
            AgentComponent::EncoderWorker,
            fixture.inputs(TickOutcome::Progress),
        );
    });
    commands
        .send(WorkerCommand::StartRecording {
            request: Box::new(recording_start_request()),
            policy: RecordingStartPolicy::default(),
        })
        .map_err(|_| "worker unavailable")?;
    let keep_busy = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let producer = busy.then(|| {
        let running = keep_busy.clone();
        let sender = commands.clone();
        thread::spawn(move || {
            while running.load(std::sync::atomic::Ordering::Acquire) {
                match sender.try_send(WorkerCommand::RefreshTrayConnectionStatus) {
                    Ok(()) => {}
                    Err(mpsc::TrySendError::Full(_)) => thread::yield_now(),
                    Err(mpsc::TrySendError::Disconnected(_)) => break,
                }
            }
        })
    });
    let started = outbound.recv_timeout(Duration::from_secs(2));
    let tick = outbound.recv_timeout(Duration::from_secs(2));
    keep_busy.store(false, std::sync::atomic::Ordering::Release);
    if let Some(producer) = producer {
        producer.join().map_err(|_| "producer failed")?;
    }
    commands
        .send(WorkerCommand::Stop)
        .map_err(|_| "worker stopped early")?;
    worker.join().map_err(|_| "worker failed")?;
    assert!(
        matches!(started, Ok(AgentOutboundEvent::CommandEvent(event)) if matches!(&*event, crate::app::responses::NativeHostCommandEvent::RecordingProgress(progress) if progress.duration_ms == 0))
    );
    assert!(
        matches!(tick, Ok(AgentOutboundEvent::CommandEvent(event)) if matches!(&*event, crate::app::responses::NativeHostCommandEvent::RecordingProgress(progress) if progress.duration_ms == 1000))
    );
    Ok(())
}
