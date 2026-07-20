//! Windows process-log helpers for app host modes.
#![cfg(all(windows, not(test)))]

use std::io::Write;

use super::{ModeError, ProcessMode};

pub(super) fn write_native_host_run_error_log(
    logger: &mut crate::diagnostics::ProcessLogger,
    error: &crate::app::bridge::NativeHostRunError,
) {
    let error_fields = crate::diagnostics::LogErrorFields {
        code: native_host_run_error_code(error).to_owned(),
        recoverable: true,
    };
    let message = format!("native host bridge failed: {error:?}");
    let _ = logger.write_event_now(crate::diagnostics::ProcessLogEvent {
        level: crate::diagnostics::LogLevel::Error,
        target: "sniptale_native_agent::native_host",
        operation: "bridge",
        operation_id: None,
        phase: "bridge-run",
        message: &message,
        duration_ms: None,
        error: Some(&error_fields),
    });
}

pub(super) fn write_agent_run_error_log(
    logger: &mut crate::diagnostics::ProcessLogger,
    error: &crate::app::resident::AgentRunError,
) {
    let error_fields = crate::diagnostics::LogErrorFields {
        code: agent_run_error_code(error).to_owned(),
        recoverable: true,
    };
    let message = format!("resident agent failed: {error:?}");
    logger
        .write_event_now(crate::diagnostics::ProcessLogEvent {
            level: crate::diagnostics::LogLevel::Error,
            target: "sniptale_native_agent::agent_runtime",
            operation: "startup",
            operation_id: None,
            phase: "agent-run",
            message: &message,
            duration_ms: None,
            error: Some(&error_fields),
        })
        .unwrap_or(());
}

pub(super) fn write_agent_startup_recovery_logs(logger: &mut crate::diagnostics::ProcessLogger) {
    if let Ok(store) = crate::recording::RecordingTransferStore::open_default()
        && let Ok(summary) = store.startup_recovery_summary()
    {
        logger
            .write_startup_recording_recovery_summary_now(
                summary.pending_recordings,
                summary.keep_writing,
                summary.retained_rejected,
                summary.deleted,
            )
            .unwrap_or(());
    }
    if let Ok(store) = crate::capture::ScreenshotTransferStore::open_default()
        && let Ok(summary) = store.startup_recovery_summary()
    {
        logger
            .write_startup_screenshot_recovery_summary_now(
                summary.pending_screenshots,
                summary.keep_writing,
                summary.retained_rejected,
                summary.deleted,
            )
            .unwrap_or(());
    }
}

pub(super) fn write_logger_startup_warning(
    stderr: &mut impl Write,
    mode: ProcessMode,
    error: crate::diagnostics::ProcessLoggerError,
) -> Result<(), ModeError> {
    let message = crate::diagnostics::logger_startup_warning_line(
        mode.as_str(),
        logger_startup_error_code(error),
    );
    stderr
        .write_all(message.as_bytes())
        .map_err(|_| ModeError::Io)
}

fn agent_run_error_code(error: &crate::app::resident::AgentRunError) -> &'static str {
    match error {
        crate::app::resident::AgentRunError::StartTimeout(_) => "agent.start_timeout",
        crate::app::resident::AgentRunError::EventChannelClosed => "agent.event_channel_closed",
        crate::app::resident::AgentRunError::StartFailed(_) => "agent.start_failed",
        crate::app::resident::AgentRunError::ResidentSingletonUnavailable => {
            "agent.resident_singleton_unavailable"
        }
        crate::app::resident::AgentRunError::CommandChannelClosed(_) => {
            "agent.command_channel_closed"
        }
        crate::app::resident::AgentRunError::JoinFailed(_) => "agent.join_failed",
    }
}

fn native_host_run_error_code(error: &crate::app::bridge::NativeHostRunError) -> &'static str {
    match error {
        crate::app::bridge::NativeHostRunError::AuthenticatedBridge(error) => {
            authenticated_bridge_error_code(error)
        }
        crate::app::bridge::NativeHostRunError::InstallIdentityDrift(_) => {
            "native_host.install_identity_drift"
        }
        crate::app::bridge::NativeHostRunError::PipeAuthenticationUnavailable => {
            "native_host.pipe_auth_unavailable"
        }
    }
}

fn authenticated_bridge_error_code(
    error: &crate::ipc::AuthenticatedBridgeForwardError,
) -> &'static str {
    match error {
        crate::ipc::AuthenticatedBridgeForwardError::Handshake(_) => "native_host.pipe_auth",
        crate::ipc::AuthenticatedBridgeForwardError::Split(_) => "native_host.pipe_split",
        crate::ipc::AuthenticatedBridgeForwardError::Forward(_) => "native_host.forward",
    }
}

const fn logger_startup_error_code(error: crate::diagnostics::ProcessLoggerError) -> &'static str {
    match error {
        crate::diagnostics::ProcessLoggerError::Setup(_) => "logger.setup",
        crate::diagnostics::ProcessLoggerError::File(_) => "logger.file",
        crate::diagnostics::ProcessLoggerError::Clock => "logger.clock",
        crate::diagnostics::ProcessLoggerError::Json => "logger.json",
        crate::diagnostics::ProcessLoggerError::Io => "logger.io",
    }
}
