//! Resident tray-agent lifecycle owner.

#[cfg(any(windows, test))]
mod assets_worker;
#[cfg(any(windows, test))]
mod audio_worker;
#[cfg(any(windows, test))]
mod capture_worker;
#[cfg(any(windows, test))]
mod command_events;
#[cfg(any(windows, test))]
pub(crate) mod control;
#[cfg(any(windows, test))]
mod effects;
#[cfg(any(windows, test))]
mod events;
#[cfg(any(windows, test))]
mod hotkeys;
#[cfg(windows)]
mod pipe;
#[cfg(any(windows, test))]
pub(crate) mod pipe_broker;
#[cfg(any(windows, test))]
mod pipe_peer;
#[cfg(windows)]
pub(crate) mod pipe_windows;
#[cfg(any(windows, test))]
mod recording_worker;
#[cfg(test)]
pub(crate) mod shutdown;
mod startup;
#[cfg(any(windows, test))]
mod telemetry_worker;
#[cfg(any(windows, test))]
mod transfer;
#[cfg(any(windows, test))]
mod tray;
#[cfg(any(windows, test))]
mod worker_runtime;
#[cfg(any(windows, test))]
mod worker_start;
#[cfg(any(windows, test))]
mod workers;

#[cfg(any(windows, test))]
use std::{
    sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender},
    time::Duration,
};

#[cfg(test)]
pub(crate) use events::AgentEventPublishResult;
#[cfg(any(windows, test))]
pub(crate) use events::{AgentEventHub, AgentOutboundEvent};
#[cfg(any(windows, test))]
use workers::{start_workers, stop_workers, wait_for_worker_starts};

#[cfg(any(windows, test))]
const WORKER_START_TIMEOUT: Duration = Duration::from_secs(5);
#[cfg(any(windows, test))]
const SHUTDOWN_POLL_TIMEOUT: Duration = Duration::from_millis(500);
#[cfg(any(windows, test))]
const RUNTIME_APPLY_ACK_TIMEOUT: Duration = Duration::from_secs(5);
#[cfg(any(windows, test))]
use self::startup::runtime_agent_startup_inputs;
#[cfg(any(windows, test))]
pub(crate) use self::startup::{AgentComponent, AgentStartupInputs, plan_agent_start};
pub(crate) use self::startup::{AgentStartupStatus, runtime_agent_startup_plan};

#[cfg(not(test))]
#[cfg(any(windows, test))]
type AgentProcessLogSink = crate::diagnostics::ProcessLogSink;
#[cfg(test)]
type AgentProcessLogSink = ();

#[cfg(any(windows, test))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AgentShutdownSignal {
    Stop,
}

#[cfg(any(windows, test))]
pub(super) enum WorkerCommand {
    ApplyHotkeyPlan {
        plan: crate::hotkeys::HotkeyRegistrationPlan,
        route: crate::hotkeys::HotkeyPlanRoute,
        result_sender: SyncSender<Result<(), crate::hotkeys::HotkeyRuntimeError>>,
    },
    ManualHotkeyAction {
        action_id: String,
    },
    RestoreHotkeys {
        result_sender: SyncSender<Result<(), crate::hotkeys::HotkeyRuntimeError>>,
    },
    ApplyTrayRegistry {
        registry: crate::tray::TrayActionRegistry,
        result_sender: SyncSender<Result<(), crate::tray::TrayRuntimeError>>,
    },
    ShowTrayNotice {
        kind: crate::tray::TrayNoticeKind,
    },
    UpdateTrayRecordingStatus {
        status: crate::tray::TrayRecordingStatus,
    },
    RefreshTrayConnectionStatus,
    #[cfg_attr(not(windows), allow(dead_code))]
    FenceMode {
        result_sender: SyncSender<bool>,
    },
    PrepareManualAsset {
        job: Box<crate::assets::AssetJob>,
        result_sender: SyncSender<Result<(), crate::assets::AssetPublishServiceError>>,
    },
    PublishManualAsset {
        job_id: crate::assets::AssetJobId,
        source: Box<dyn crate::assets::FinalizedArtifactSource>,
    },
    CaptureScreenshot {
        request: Box<crate::capture::ScreenshotCaptureRequest>,
        extension_controller_lease_id: Option<String>,
    },
    StartRecording {
        request: Box<crate::recording::RecordingStartRequest>,
        policy: crate::recording::RecordingStartPolicy,
    },
    ControlRecording {
        request: Box<crate::recording::RecordingControlRequest>,
    },
    ControlActiveRecording {
        control: crate::protocol::RecordingControl,
    },
    StartRecordingTelemetry {
        policy: crate::telemetry::RecordingTelemetryPolicy,
        recording_id: String,
        started_at_monotonic_ns: u64,
    },
    PauseRecordingTelemetry {
        recording_id: String,
        paused_at_monotonic_ns: u64,
    },
    ResumeRecordingTelemetry {
        recording_id: String,
        resumed_at_monotonic_ns: u64,
    },
    FinishRecordingTelemetry {
        recording_id: String,
        stopped_at_monotonic_ns: u64,
    },
    CancelRecordingTelemetry {
        recording_id: String,
        cancelled_at_monotonic_ns: u64,
    },
    DiscardRecordingTelemetry {
        recording_id: String,
    },
    SnapshotRecordingTelemetry {
        recording_id: String,
        duration_ms: u64,
        result_sender: SyncSender<Option<crate::telemetry::SanitizedRecordingTelemetrySnapshot>>,
    },
    #[cfg(test)]
    TrayMenuCommandSelected {
        command_id: u32,
    },
    Stop,
}

#[cfg(any(windows, test))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum WorkerEvent {
    Started(AgentComponent),
    Failed(AgentComponent),
}

#[cfg(any(windows, test))]
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum AgentRunError {
    StartTimeout(AgentComponent),
    EventChannelClosed,
    StartFailed(AgentComponent),
    #[cfg(all(windows, not(test)))]
    ResidentSingletonUnavailable,
    CommandChannelClosed(AgentComponent),
    JoinFailed(AgentComponent),
}

#[cfg(any(windows, test))]
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct AgentRunReport {
    pub(crate) startup_status: AgentStartupStatus,
    pub(crate) started_components: Vec<AgentComponent>,
    pub(crate) stopped_components: Vec<AgentComponent>,
}

#[cfg(any(windows, test))]
pub(crate) struct AgentRunInputs {
    pub(crate) startup: AgentStartupInputs,
    pub(crate) shutdown: Receiver<AgentShutdownSignal>,
}

#[cfg(any(windows, test))]
pub(crate) fn run_resident_agent_with_log(
    log: Option<&AgentProcessLogSink>,
) -> Result<AgentRunReport, AgentRunError> {
    #[cfg(all(windows, not(test)))]
    let _singleton = acquire_resident_agent_singleton()?;
    let (_shutdown_sender, shutdown) = mpsc::sync_channel(1);
    let inputs = AgentRunInputs {
        startup: runtime_agent_startup_inputs(),
        shutdown,
    };
    run_resident_agent_until_shutdown_with_log(&inputs, log)
}

#[cfg(all(windows, not(test)))]
fn acquire_resident_agent_singleton() -> Result<crate::ipc::WindowsPipe, AgentRunError> {
    let identity = crate::install::expected_install_identity();
    let secret = crate::install::load_current_user_pipe_secret(identity.install_id)
        .map_err(|_| AgentRunError::ResidentSingletonUnavailable)?;
    crate::ipc::create_resident_agent_singleton_guard(&secret.install_id, &secret.current_user_sid)
        .map_err(|_| AgentRunError::ResidentSingletonUnavailable)
}

#[cfg(test)]
pub(crate) fn run_resident_agent_until_shutdown(
    inputs: &AgentRunInputs,
) -> Result<AgentRunReport, AgentRunError> {
    run_resident_agent_until_shutdown_with_log(inputs, None)
}

#[cfg(any(windows, test))]
fn run_resident_agent_until_shutdown_with_log(
    inputs: &AgentRunInputs,
    log: Option<&AgentProcessLogSink>,
) -> Result<AgentRunReport, AgentRunError> {
    let plan = plan_agent_start(&inputs.startup);
    let (event_sender, event_receiver) = mpsc::sync_channel(plan.components.len());
    let (shutdown_sender, shutdown_receiver) = mpsc::sync_channel(1);
    let mut workers = start_workers(
        &plan.components,
        plan.status,
        &inputs.startup.active_settings,
        &event_sender,
        &shutdown_sender,
        log,
    );
    let started_components = wait_for_worker_starts(&plan.components, &event_receiver)?;

    wait_for_shutdown_request(&inputs.shutdown, &shutdown_receiver)?;

    let stopped_components = stop_workers(&mut workers)?;
    Ok(AgentRunReport {
        startup_status: plan.status,
        started_components,
        stopped_components,
    })
}

#[cfg(any(windows, test))]
fn wait_for_shutdown_request(
    external: &Receiver<AgentShutdownSignal>,
    internal: &Receiver<AgentShutdownSignal>,
) -> Result<(), AgentRunError> {
    loop {
        match internal.try_recv() {
            Ok(AgentShutdownSignal::Stop) | Err(mpsc::TryRecvError::Disconnected) => return Ok(()),
            Err(mpsc::TryRecvError::Empty) => {}
        }
        match external.recv_timeout(SHUTDOWN_POLL_TIMEOUT) {
            Ok(AgentShutdownSignal::Stop) | Err(RecvTimeoutError::Disconnected) => return Ok(()),
            Err(RecvTimeoutError::Timeout) => {}
        }
    }
}
