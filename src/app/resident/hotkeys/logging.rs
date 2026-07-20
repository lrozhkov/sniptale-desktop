//! Redacted hotkey-worker observability and user-notice projection.

use std::sync::mpsc::SyncSender;

use super::super::{WorkerCommand, events::AgentEventPublishResult};

pub(super) fn log_hotkey_publish_result(
    log: Option<&super::super::AgentProcessLogSink>,
    result: AgentEventPublishResult,
) {
    match result {
        AgentEventPublishResult::Queued => log_hotkey_event(
            log,
            "event-queued",
            "hotkey event queued for browser delivery",
            None,
        ),
        AgentEventPublishResult::NoSubscriber => log_hotkey_event(
            log,
            "event-no-subscriber",
            "hotkey event could not be queued because no browser subscriber is connected",
            Some("hotkeys.no_subscriber"),
        ),
        AgentEventPublishResult::QueueFull => log_hotkey_event(
            log,
            "event-queue-full",
            "hotkey event could not be queued because browser delivery queue is full",
            Some("hotkeys.queue_full"),
        ),
    }
}

#[cfg(not(test))]
pub(super) fn log_hotkey_event(
    log: Option<&super::super::AgentProcessLogSink>,
    phase: &'static str,
    message: &str,
    code: Option<&str>,
) {
    let Some(log) = log else { return };
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
        target: "sniptale_native_agent::hotkeys",
        operation: "hotkeys",
        operation_id: None,
        phase,
        message,
        duration_ms: None,
        error: error.as_ref(),
    });
}

#[cfg(test)]
pub(super) fn log_hotkey_event(
    _log: Option<&super::super::AgentProcessLogSink>,
    _phase: &'static str,
    _message: &str,
    _code: Option<&str>,
) {
}

pub(super) fn show_notice_if_not_queued(
    publish_result: AgentEventPublishResult,
    tray_command_sender: Option<&SyncSender<WorkerCommand>>,
) {
    let notice = match publish_result {
        AgentEventPublishResult::Queued => None,
        AgentEventPublishResult::NoSubscriber => {
            Some(crate::tray::TrayNoticeKind::ExtensionDisconnected)
        }
        AgentEventPublishResult::QueueFull => Some(crate::tray::TrayNoticeKind::ActionRejected),
    };
    if let (Some(notice), Some(sender)) = (notice, tray_command_sender) {
        let _ = sender.try_send(WorkerCommand::ShowTrayNotice { kind: notice });
    }
}
