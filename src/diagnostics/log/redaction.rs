//! Diagnostic output redaction.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DiagnosticOutput {
    Log,
    DoctorJson,
    CollectDiagnostics,
    OperationFailure,
    SupportBundle,
}

const REDACTED_DIAGNOSTIC_OUTPUTS: &[DiagnosticOutput] = &[
    DiagnosticOutput::Log,
    DiagnosticOutput::DoctorJson,
    DiagnosticOutput::CollectDiagnostics,
    DiagnosticOutput::OperationFailure,
    DiagnosticOutput::SupportBundle,
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RedactedDiagnostic {
    pub(crate) output: DiagnosticOutput,
    pub(crate) operation: &'static str,
    pub(crate) phase: &'static str,
    pub(crate) message: String,
}

pub(crate) fn redact_for_output(
    output: DiagnosticOutput,
    input: &str,
    operation: &'static str,
    phase: &'static str,
) -> RedactedDiagnostic {
    debug_assert!(REDACTED_DIAGNOSTIC_OUTPUTS.contains(&output));
    RedactedDiagnostic {
        output,
        operation,
        phase,
        message: if output == DiagnosticOutput::OperationFailure {
            redact_operation_failure_message(input).unwrap_or_else(|| redact_message(input))
        } else {
            redact_message(input)
        },
    }
}

fn redact_operation_failure_message(input: &str) -> Option<String> {
    let serde_json::Value::Object(fields) = serde_json::from_str(input).ok()? else {
        return None;
    };
    if fields.get("type").and_then(serde_json::Value::as_str) != Some("app.operation.failed") {
        return None;
    }

    let operation = fields
        .get("operation")
        .and_then(serde_json::Value::as_str)
        .map_or_else(|| "unknown".to_owned(), sanitize_diagnostic_label);
    let phase = fields
        .get("phase")
        .and_then(serde_json::Value::as_str)
        .map_or_else(|| "unknown".to_owned(), sanitize_diagnostic_label);
    let error_code = fields
        .get("error")
        .and_then(serde_json::Value::as_object)
        .and_then(|error| error.get("code"))
        .and_then(serde_json::Value::as_str)
        .map_or_else(|| "unknown".to_owned(), sanitize_diagnostic_label);

    Some(format!(
        "[redacted:operation-failure message-type=app.operation.failed operation={operation} phase={phase} error-code={error_code}]"
    ))
}

fn redact_message(input: &str) -> String {
    if let Some(message_type) = extract_native_message_type(input) {
        return format!("[redacted:raw-payload message-type={message_type}]");
    }
    if input.contains(";base64,") || input.contains("base64,") {
        return "[redacted:base64-media]".to_owned();
    }
    if input.contains("typed text:") || input.contains("clipboard:") {
        return "[redacted:typed-text] [redacted:clipboard]".to_owned();
    }
    if contains_auth_material(input) {
        return "[redacted:auth-header] [redacted:cookie]".to_owned();
    }
    if looks_like_local_path(input) {
        return "[path-category:local]".to_owned();
    }
    if contains_extension_origin(input) {
        return "[redacted:extension-origin]".to_owned();
    }
    if contains_extension_id(input) {
        return "[redacted:extension-id]".to_owned();
    }
    if contains_media_identifier(input) {
        return "[redacted:media-id]".to_owned();
    }
    redact_inline_sensitive_text(input)
}

fn extract_native_message_type(input: &str) -> Option<String> {
    if let Ok(serde_json::Value::Object(fields)) = serde_json::from_str(input)
        && let Some(serde_json::Value::String(message_type)) = fields.get("type")
    {
        return Some(sanitize_message_type(message_type));
    }

    let marker = "\"type\":\"";
    let start = input.find(marker)? + marker.len();
    let rest = &input[start..];
    let end = rest.find('"')?;
    Some(sanitize_message_type(&rest[..end]))
}

fn sanitize_message_type(message_type: &str) -> String {
    sanitize_diagnostic_label_with_limit(message_type, 80)
}

fn sanitize_diagnostic_label(value: &str) -> String {
    sanitize_diagnostic_label_with_limit(value, 64)
}

fn sanitize_diagnostic_label_with_limit(value: &str, max_len: usize) -> String {
    if value.is_empty()
        || value.len() > max_len
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
    {
        "unknown".to_owned()
    } else {
        value.to_owned()
    }
}

fn contains_auth_material(input: &str) -> bool {
    let lower = input.to_ascii_lowercase();
    lower.contains("authorization:") || lower.contains("cookie:")
}

fn looks_like_local_path(input: &str) -> bool {
    input.contains(":\\Users\\")
        || input.contains(":/Users/")
        || input.contains("\\AppData\\")
        || input.contains("/AppData/")
        || input.contains("/home/")
        || input.contains("/Users/")
        || input.contains(":\\ProgramData\\")
        || input.contains(":/ProgramData/")
}

fn contains_extension_origin(input: &str) -> bool {
    input.to_ascii_lowercase().contains("chrome-extension://")
}

fn contains_extension_id(input: &str) -> bool {
    input
        .split(|character: char| !character.is_ascii_alphanumeric())
        .any(is_chrome_extension_id)
}

fn is_chrome_extension_id(value: &str) -> bool {
    value.len() == 32
        && value
            .bytes()
            .all(|byte| matches!(byte, b'a'..=b'p' | b'A'..=b'P'))
}

fn contains_media_identifier(input: &str) -> bool {
    let lower = input.to_ascii_lowercase();
    lower.contains("recordingid")
        || lower.contains("recording_id")
        || lower.contains("operationid")
        || lower.contains("operation_id")
        || lower.contains("screenshotid")
        || lower.contains("screenshot_id")
        || lower.contains("captureid")
        || lower.contains("capture_id")
        || lower.contains("recording-")
        || lower.contains("recording_")
        || lower.contains("rec-")
        || lower.contains("screenshot-")
        || lower.contains("screenshot_")
        || lower.contains("capture-")
        || lower.contains("capture_")
}

fn redact_inline_sensitive_text(input: &str) -> String {
    let lower = input.to_ascii_lowercase();
    if lower.contains("http://") || lower.contains("https://") {
        "[redacted:url]".to_owned()
    } else if lower.contains("token") {
        "[redacted:token]".to_owned()
    } else {
        input.to_owned()
    }
}
