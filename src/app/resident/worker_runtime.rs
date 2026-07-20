//! Resident worker runtime factory wiring.

use std::sync::mpsc::SyncSender;

#[cfg(windows)]
use super::telemetry_worker;
use super::{AgentEventHub, AgentProcessLogSink, WorkerCommand};

#[cfg(windows)]
pub(super) fn default_capture_runner(
    _log: Option<&AgentProcessLogSink>,
) -> Option<Box<dyn crate::capture::ScreenshotCaptureRunner>> {
    crate::capture::ScreenshotTransferStore::open_default()
        .ok()
        .and_then(crate::capture::default_screenshot_capture_runner)
}

#[cfg(not(windows))]
pub(super) fn default_capture_runner(
    _log: Option<&AgentProcessLogSink>,
) -> Option<Box<dyn crate::capture::ScreenshotCaptureRunner>> {
    None
}

#[cfg(windows)]
pub(super) fn default_recording_runner(
    _log: Option<&AgentProcessLogSink>,
    telemetry_command_sender: Option<SyncSender<WorkerCommand>>,
    agent_events: AgentEventHub,
) -> Option<Box<dyn crate::recording::RecordingRunner>> {
    crate::recording::RecordingTransferStore::open_default()
        .ok()
        .and_then(|store| {
            crate::recording::default_recording_runner_with_telemetry(
                store,
                telemetry_worker::TelemetryWorkerCommandSink::new(
                    telemetry_command_sender,
                    agent_events,
                ),
            )
        })
}

#[cfg(not(windows))]
pub(super) fn default_recording_runner(
    _log: Option<&AgentProcessLogSink>,
    _telemetry_command_sender: Option<SyncSender<WorkerCommand>>,
    _agent_events: AgentEventHub,
) -> Option<Box<dyn crate::recording::RecordingRunner>> {
    None
}

#[cfg(windows)]
pub(super) fn default_telemetry_source() -> Box<dyn crate::telemetry::RecordingTelemetrySource> {
    Box::new(crate::telemetry::default_recording_telemetry_source())
}

#[cfg(not(windows))]
pub(super) fn default_telemetry_source() -> Box<dyn crate::telemetry::RecordingTelemetrySource> {
    Box::new(crate::telemetry::NoopRecordingTelemetrySource)
}
