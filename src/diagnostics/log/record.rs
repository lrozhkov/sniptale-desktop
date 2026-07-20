//! Structured JSON-lines log record shape.

use serde::Serialize;

use super::{DiagnosticOutput, redact_for_output};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

const LOG_LEVELS: &[LogLevel] = &[
    LogLevel::Trace,
    LogLevel::Debug,
    LogLevel::Info,
    LogLevel::Warn,
    LogLevel::Error,
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct StructuredLogContext {
    pub(crate) app_version: String,
    pub(crate) protocol_version: u16,
    pub(crate) settings_schema_version: u16,
    pub(crate) app_cache_schema_version: u16,
    pub(crate) platform_kind: String,
    pub(crate) process_mode: String,
    pub(crate) process_id: u32,
    pub(crate) connection_id: Option<String>,
    pub(crate) controller_lease_id: Option<String>,
    pub(crate) operation: &'static str,
    pub(crate) operation_id: Option<String>,
    pub(crate) phase: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LogErrorFields {
    pub(crate) code: String,
    pub(crate) recoverable: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StructuredLogRecord {
    pub(crate) timestamp_epoch_ms: u64,
    pub(crate) level: &'static str,
    pub(crate) target: String,
    pub(crate) app_version: String,
    pub(crate) protocol_version: u16,
    pub(crate) settings_schema_version: u16,
    pub(crate) app_cache_schema_version: u16,
    pub(crate) platform: PlatformLogFields,
    pub(crate) process_mode: String,
    pub(crate) process_role: String,
    pub(crate) process_id: u32,
    pub(crate) connection_id: Option<String>,
    pub(crate) controller_lease_id: Option<String>,
    pub(crate) operation: &'static str,
    pub(crate) operation_id: Option<String>,
    pub(crate) phase: &'static str,
    pub(crate) duration_ms: Option<u64>,
    pub(crate) error: Option<LogErrorFields>,
    pub(crate) message: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PlatformLogFields {
    pub(crate) kind: String,
}

pub(crate) fn build_structured_log_record(
    timestamp_epoch_ms: u64,
    level: LogLevel,
    target: &str,
    context: StructuredLogContext,
    message: &str,
    duration_ms: Option<u64>,
    error: Option<LogErrorFields>,
) -> StructuredLogRecord {
    let process_role = process_role_for_mode(&context.process_mode).to_owned();
    let redacted = redact_for_output(
        DiagnosticOutput::Log,
        message,
        context.operation,
        context.phase,
    );
    StructuredLogRecord {
        timestamp_epoch_ms,
        level: level.as_str(),
        target: target.to_owned(),
        app_version: context.app_version,
        protocol_version: context.protocol_version,
        settings_schema_version: context.settings_schema_version,
        app_cache_schema_version: context.app_cache_schema_version,
        platform: PlatformLogFields {
            kind: context.platform_kind,
        },
        process_mode: context.process_mode,
        process_role,
        process_id: context.process_id,
        connection_id: redact_identifier(context.connection_id, DiagnosticIdentifier::Connection),
        controller_lease_id: redact_identifier(
            context.controller_lease_id,
            DiagnosticIdentifier::ControllerLease,
        ),
        operation: context.operation,
        operation_id: redact_identifier(context.operation_id, DiagnosticIdentifier::Operation),
        phase: context.phase,
        duration_ms,
        error,
        message: redacted.message,
    }
}

fn process_role_for_mode(process_mode: &str) -> &'static str {
    match process_mode {
        "agent" => "resident-agent",
        "native-host" => "native-host",
        _ => "support",
    }
}

pub(crate) fn serialize_log_record_json_line(
    record: &StructuredLogRecord,
) -> Result<String, serde_json::Error> {
    let mut line = serde_json::to_string(record)?;
    line.push('\n');
    Ok(line)
}

impl LogLevel {
    fn as_str(self) -> &'static str {
        debug_assert!(LOG_LEVELS.contains(&self));
        match self {
            Self::Trace => "trace",
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
        }
    }
}

#[derive(Clone, Copy)]
enum DiagnosticIdentifier {
    Connection,
    ControllerLease,
    Operation,
}

fn redact_identifier(value: Option<String>, identifier: DiagnosticIdentifier) -> Option<String> {
    value.map(|raw| match identifier {
        DiagnosticIdentifier::Connection => "[redacted:connection-id]".to_owned(),
        DiagnosticIdentifier::ControllerLease => "[redacted:controller-lease-id]".to_owned(),
        DiagnosticIdentifier::Operation => redacted_operation_id(&raw),
    })
}

fn redacted_operation_id(value: &str) -> String {
    let lower = value.to_ascii_lowercase();
    if lower.contains("recording") || lower.starts_with("rec-") {
        "[redacted:media-id kind=recording]".to_owned()
    } else if lower.contains("capture") || lower.contains("screenshot") {
        "[redacted:media-id kind=screenshot]".to_owned()
    } else {
        "[redacted:operation-id]".to_owned()
    }
}
