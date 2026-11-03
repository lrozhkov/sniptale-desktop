//! Shared report accounting for concurrent resident pipe sessions.

use std::{
    sync::{Arc, Mutex},
    thread,
};

use crate::app::resident::pipe_broker::{AgentPipeServerError, AgentPipeServerReport};

pub(super) fn report_snapshot(
    report: &Arc<Mutex<AgentPipeServerReport>>,
) -> Result<AgentPipeServerReport, AgentPipeServerError> {
    report
        .lock()
        .map(|report| *report)
        .map_err(|_| AgentPipeServerError::Accept)
}

pub(super) fn increment_accepted(
    report: &Arc<Mutex<AgentPipeServerReport>>,
) -> Result<usize, AgentPipeServerError> {
    let mut report = report.lock().map_err(|_| AgentPipeServerError::Accept)?;
    report.accepted += 1;
    Ok(report.accepted)
}

pub(super) fn increment_completed(
    report: &Arc<Mutex<AgentPipeServerReport>>,
) -> Result<usize, AgentPipeServerError> {
    let mut report = report.lock().map_err(|_| AgentPipeServerError::Accept)?;
    report.completed += 1;
    Ok(report.completed)
}

pub(super) fn increment_failed(
    report: &Arc<Mutex<AgentPipeServerReport>>,
) -> Result<usize, AgentPipeServerError> {
    let mut report = report.lock().map_err(|_| AgentPipeServerError::Accept)?;
    report.failed += 1;
    Ok(report.failed)
}

pub(super) fn increment_startup_failures(
    report: &Arc<Mutex<AgentPipeServerReport>>,
) -> Result<usize, AgentPipeServerError> {
    let mut report = report.lock().map_err(|_| AgentPipeServerError::Accept)?;
    report.startup_failures += 1;
    Ok(report.startup_failures)
}

pub(super) fn join_finished_sessions<T>(
    handles: &mut Vec<thread::ScopedJoinHandle<'_, T>>,
    report: &Arc<Mutex<AgentPipeServerReport>>,
) -> Result<(), AgentPipeServerError> {
    while let Some(index) = handles
        .iter()
        .position(thread::ScopedJoinHandle::is_finished)
    {
        if handles.swap_remove(index).join().is_err() {
            increment_failed(report)?;
        }
    }
    Ok(())
}

pub(super) fn record_session_accepted(
    report: &Arc<Mutex<AgentPipeServerReport>>,
    log: Option<&super::super::logging::AgentPipeServerLogSink>,
) -> Result<(), AgentPipeServerError> {
    let accepted = increment_accepted(report)?;
    tracing::info!(
        target: "sniptale_native_agent::resident_pipe",
        accepted,
        "resident pipe accepted native-host session"
    );
    super::super::logging::log_resident_pipe_event(
        log,
        "session-accepted",
        "resident pipe accepted native-host session",
        None,
    );
    Ok(())
}
