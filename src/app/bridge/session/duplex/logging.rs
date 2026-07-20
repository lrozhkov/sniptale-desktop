//! Safe resident pipe session diagnostics.

use serde_json::Value;

#[cfg(not(test))]
pub(crate) type AgentPipeSessionLogSink = crate::diagnostics::ProcessLogSink;
#[cfg(test)]
pub(crate) type AgentPipeSessionLogSink = ();

#[cfg(not(test))]
pub(super) fn clone_log_sink(
    log: Option<&AgentPipeSessionLogSink>,
) -> Option<AgentPipeSessionLogSink> {
    log.cloned()
}

#[cfg(test)]
pub(super) fn clone_log_sink(
    log: Option<&AgentPipeSessionLogSink>,
) -> Option<AgentPipeSessionLogSink> {
    log.copied()
}

pub(super) fn response_type_summary(responses: &[Value]) -> String {
    let mut types = Vec::new();
    for response in responses.iter().take(8) {
        let response_type = response
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        if response_type == "app.operation.failed" {
            let operation = response
                .get("operation")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            let phase = response
                .get("phase")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            let code = response
                .get("error")
                .and_then(|error| error.get("code"))
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            types.push(format!("{response_type}:{operation}:{phase}:{code}"));
        } else {
            types.push(response_type.to_owned());
        }
    }
    if responses.len() > types.len() {
        types.push("truncated".to_owned());
    }
    types.join(",")
}

#[cfg(not(test))]
pub(super) fn log_pipe_session_event(
    log: Option<&AgentPipeSessionLogSink>,
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
pub(super) fn log_pipe_session_event(
    _log: Option<&AgentPipeSessionLogSink>,
    _phase: &'static str,
    _message: &str,
    _code: Option<&str>,
) {
}
