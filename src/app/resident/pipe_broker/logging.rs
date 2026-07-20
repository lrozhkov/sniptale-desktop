//! Resident pipe server structured event logging.

#[cfg(not(test))]
pub(crate) type AgentPipeServerLogSink = crate::diagnostics::ProcessLogSink;
#[cfg(test)]
pub(crate) type AgentPipeServerLogSink = ();

#[cfg(not(test))]
pub(crate) fn clone_log_sink(
    log: Option<&AgentPipeServerLogSink>,
) -> Option<AgentPipeServerLogSink> {
    log.cloned()
}

#[cfg(test)]
pub(crate) fn clone_log_sink(
    log: Option<&AgentPipeServerLogSink>,
) -> Option<AgentPipeServerLogSink> {
    log.copied()
}

#[cfg(not(test))]
pub(crate) fn log_resident_pipe_event(
    log: Option<&AgentPipeServerLogSink>,
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
        operation: "native-host-session",
        operation_id: None,
        phase,
        message,
        duration_ms: None,
        error: error.as_ref(),
    });
}

#[cfg(test)]
pub(crate) fn log_resident_pipe_event(
    _log: Option<&AgentPipeServerLogSink>,
    _phase: &'static str,
    _message: &str,
    _code: Option<&str>,
) {
}
