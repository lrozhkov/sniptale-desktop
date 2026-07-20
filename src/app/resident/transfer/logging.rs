//! Category-only process log events for transfer cleanup.

#[cfg(all(windows, not(test)))]
pub(super) fn transfer_log(
    cleaner: &super::DefaultTransferCacheCleaner,
) -> Option<&super::super::AgentProcessLogSink> {
    cleaner.log.as_ref()
}

#[cfg(any(not(windows), test))]
pub(super) fn transfer_log(
    _cleaner: &super::DefaultTransferCacheCleaner,
) -> Option<&super::super::AgentProcessLogSink> {
    None
}

#[cfg(all(windows, not(test)))]
pub(super) fn log_transfer_cleanup_warning(
    log: Option<&super::super::AgentProcessLogSink>,
    phase: &'static str,
    message: &'static str,
    code: &'static str,
) {
    let Some(log) = log else {
        return;
    };
    let error = crate::diagnostics::LogErrorFields {
        code: code.to_owned(),
        recoverable: true,
    };
    log.write_event_now(crate::diagnostics::ProcessLogEvent {
        level: crate::diagnostics::LogLevel::Warn,
        target: "sniptale_native_agent::transfer_worker",
        operation: "transfer-cleanup",
        operation_id: None,
        phase,
        message,
        duration_ms: None,
        error: Some(&error),
    })
    .unwrap_or(());
}

#[cfg(any(not(windows), test))]
pub(super) fn log_transfer_cleanup_warning(
    _log: Option<&super::super::AgentProcessLogSink>,
    _phase: &'static str,
    _message: &'static str,
    _code: &'static str,
) {
}

#[cfg(all(windows, not(test)))]
pub(super) fn log_screenshot_cleanup_summary(
    log: Option<&super::super::AgentProcessLogSink>,
    pending: usize,
    keep_writing: usize,
    retained_rejected: usize,
    deleted: usize,
) {
    log_transfer_cleanup_summary_counts(
        log,
        "screenshot-cleanup",
        "screenshot transfer cleanup summary",
        pending,
        keep_writing,
        retained_rejected,
        deleted,
    );
}

#[cfg(any(not(windows), test))]
pub(super) fn log_screenshot_cleanup_summary(
    _log: Option<&super::super::AgentProcessLogSink>,
    _pending: usize,
    _keep_writing: usize,
    _retained_rejected: usize,
    _deleted: usize,
) {
}

#[cfg(all(windows, not(test)))]
pub(super) fn log_recording_cleanup_summary(
    log: Option<&super::super::AgentProcessLogSink>,
    pending: usize,
    keep_writing: usize,
    retained_rejected: usize,
    deleted: usize,
) {
    log_transfer_cleanup_summary_counts(
        log,
        "recording-cleanup",
        "recording transfer cleanup summary",
        pending,
        keep_writing,
        retained_rejected,
        deleted,
    );
}

#[cfg(any(not(windows), test))]
pub(super) fn log_recording_cleanup_summary(
    _log: Option<&super::super::AgentProcessLogSink>,
    _pending: usize,
    _keep_writing: usize,
    _retained_rejected: usize,
    _deleted: usize,
) {
}

#[cfg(all(windows, not(test)))]
fn log_transfer_cleanup_summary_counts(
    log: Option<&super::super::AgentProcessLogSink>,
    phase: &'static str,
    label: &'static str,
    pending: usize,
    keep_writing: usize,
    retained_rejected: usize,
    deleted: usize,
) {
    let Some(log) = log else {
        return;
    };
    let message = format!(
        "{label} pending={pending} keepWriting={keep_writing} retainedRejected={retained_rejected} deleted={deleted}",
    );
    log.write_event_now(crate::diagnostics::ProcessLogEvent {
        level: crate::diagnostics::LogLevel::Info,
        target: "sniptale_native_agent::transfer_worker",
        operation: "transfer-cleanup",
        operation_id: None,
        phase,
        message: &message,
        duration_ms: None,
        error: None,
    })
    .unwrap_or(());
}
