//! Category-only recording runtime diagnostics.

#[cfg(not(test))]
pub(super) fn log_recording_runtime_failure(
    log: Option<&super::super::AgentProcessLogSink>,
    phase: &'static str,
    operation_id: &str,
    error: crate::recording::RecordingRuntimeError,
) {
    let Some(log) = log else { return };
    let (code, native_code) = error.diagnostic_fields();
    let error = crate::diagnostics::LogErrorFields {
        code: code.to_owned(),
        recoverable: true,
    };
    let message = native_code.map_or_else(
        || "recording runtime failed".to_owned(),
        |code| {
            format!(
                "recording runtime failed nativeCode=0x{code:08x}",
                code = code as u32
            )
        },
    );
    let _ = log.write_event_now(crate::diagnostics::ProcessLogEvent {
        level: crate::diagnostics::LogLevel::Warn,
        target: "sniptale_native_agent::recording",
        operation: "recording",
        operation_id: Some(operation_id),
        phase,
        message: &message,
        duration_ms: None,
        error: Some(&error),
    });
}

#[cfg(test)]
pub(super) fn log_recording_runtime_failure(
    _log: Option<&super::super::AgentProcessLogSink>,
    _phase: &'static str,
    _operation_id: &str,
    _error: crate::recording::RecordingRuntimeError,
) {
}
