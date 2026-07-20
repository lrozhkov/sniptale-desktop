//! Unhandled panic logging context.

use super::record::{LogErrorFields, LogLevel};
#[cfg(test)]
use super::record::{StructuredLogContext, StructuredLogRecord, build_structured_log_record};
#[cfg(not(test))]
use super::runtime::{ProcessLogEvent, start_process_logger_for_mode_with_options};
#[cfg(test)]
use crate::protocol::{CACHE_SCHEMA_VERSION, PROTOCOL_VERSION, SETTINGS_SCHEMA_VERSION};
#[cfg(not(test))]
use std::sync::{Mutex, OnceLock};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CrashContext {
    pub(crate) process_mode: &'static str,
    pub(crate) operation: &'static str,
    pub(crate) operation_id: Option<String>,
    pub(crate) phase: &'static str,
}

impl CrashContext {
    pub(crate) fn for_mode(process_mode: &'static str) -> Self {
        Self {
            process_mode,
            operation: "startup",
            operation_id: None,
            phase: "mode-dispatch",
        }
    }

    pub(crate) fn with_phase(
        process_mode: &'static str,
        operation: &'static str,
        operation_id: Option<String>,
        phase: &'static str,
    ) -> Self {
        Self {
            process_mode,
            operation,
            operation_id,
            phase,
        }
    }
}

#[cfg(test)]
pub(crate) fn build_crash_log_record(
    timestamp_epoch_ms: u64,
    context: &CrashContext,
    message: &str,
) -> StructuredLogRecord {
    build_failure_log_record(
        timestamp_epoch_ms,
        context,
        message,
        "unhandled-panic",
        "sniptale_native_agent::panic",
    )
}

#[cfg(test)]
pub(crate) fn build_exception_log_record(
    timestamp_epoch_ms: u64,
    context: &CrashContext,
    exception_code: Option<u32>,
) -> StructuredLogRecord {
    build_failure_log_record(
        timestamp_epoch_ms,
        context,
        &exception_message(exception_code),
        "unhandled-exception",
        "sniptale_native_agent::exception",
    )
}

#[cfg(test)]
fn build_failure_log_record(
    timestamp_epoch_ms: u64,
    context: &CrashContext,
    message: &str,
    error_code: &str,
    target: &str,
) -> StructuredLogRecord {
    build_structured_log_record(
        timestamp_epoch_ms,
        LogLevel::Error,
        target,
        StructuredLogContext {
            app_version: env!("CARGO_PKG_VERSION").to_owned(),
            protocol_version: PROTOCOL_VERSION,
            settings_schema_version: SETTINGS_SCHEMA_VERSION,
            app_cache_schema_version: CACHE_SCHEMA_VERSION,
            platform_kind: platform_kind().to_owned(),
            process_mode: context.process_mode.to_owned(),
            process_id: std::process::id(),
            connection_id: None,
            controller_lease_id: None,
            operation: context.operation,
            operation_id: context.operation_id.clone(),
            phase: context.phase,
        },
        message,
        None,
        Some(LogErrorFields {
            code: error_code.to_owned(),
            recoverable: false,
        }),
    )
}

#[cfg(not(test))]
pub(crate) fn install_panic_logger_for_mode(process_mode: &'static str) {
    set_crash_context(CrashContext::for_mode(process_mode));
    #[cfg(windows)]
    super::windows_exception::install_unhandled_exception_logger();
    std::panic::set_hook(Box::new(|info| {
        let message = panic_message(info);
        write_current_failure_event("sniptale_native_agent::panic", "unhandled-panic", &message);
    }));
}

#[cfg(all(windows, not(test)))]
pub(super) fn log_unhandled_exception(exception_code: Option<u32>) {
    write_current_failure_event(
        "sniptale_native_agent::exception",
        "unhandled-exception",
        &exception_message(exception_code),
    );
}

#[cfg(not(test))]
fn write_current_failure_event(target: &str, error_code: &str, message: &str) {
    let context = current_crash_context();
    if let Ok(mut logger) =
        start_process_logger_for_mode_with_options(context.process_mode, None, None)
    {
        let error = LogErrorFields {
            code: error_code.to_owned(),
            recoverable: false,
        };
        let _ = logger.write_event_now(ProcessLogEvent {
            level: LogLevel::Error,
            target,
            operation: context.operation,
            operation_id: context.operation_id.as_deref(),
            phase: context.phase,
            message,
            duration_ms: None,
            error: Some(&error),
        });
    }
}

#[cfg(not(test))]
pub(crate) fn set_crash_context(context: CrashContext) {
    if let Ok(mut current) = crash_context().lock() {
        *current = context;
    }
}

#[cfg(not(test))]
fn current_crash_context() -> CrashContext {
    crash_context().lock().map_or_else(
        |_| CrashContext::for_mode("agent"),
        |context| context.clone(),
    )
}

#[cfg(not(test))]
fn crash_context() -> &'static Mutex<CrashContext> {
    static CONTEXT: OnceLock<Mutex<CrashContext>> = OnceLock::new();
    CONTEXT.get_or_init(|| Mutex::new(CrashContext::for_mode("agent")))
}

#[cfg(not(test))]
fn panic_message(info: &std::panic::PanicHookInfo<'_>) -> String {
    if let Some(message) = info.payload().downcast_ref::<&str>() {
        format!("unhandled panic: {message}")
    } else if let Some(message) = info.payload().downcast_ref::<String>() {
        format!("unhandled panic: {message}")
    } else {
        "unhandled panic".to_owned()
    }
}

#[cfg(any(test, all(windows, not(test))))]
fn exception_message(exception_code: Option<u32>) -> String {
    exception_code.map_or_else(
        || "unhandled native exception".to_owned(),
        |code| format!("unhandled native exception code=0x{code:08X}"),
    )
}

#[cfg(test)]
const fn platform_kind() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    }
}
