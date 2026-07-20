use serde_json::Value;

use crate::protocol::{
    errors::{ParseError, ParseErrorKind},
    versions::{MAX_SETTINGS_WARNINGS, MAX_WARNING_FIELD_CHARS, MAX_WARNING_MESSAGE_CHARS},
};

pub(super) fn validate_settings_warnings(
    key: &str,
    value: &Value,
    path: &str,
) -> Result<(), ParseError> {
    if key != "warnings" {
        return Ok(());
    }

    let warnings = value.as_array().ok_or_else(|| {
        ParseError::new(
            ParseErrorKind::MalformedJson,
            path,
            "warnings must be an array",
        )
    })?;
    if warnings.len() > MAX_SETTINGS_WARNINGS {
        return Err(ParseError::new(
            ParseErrorKind::ArrayTooLong,
            path,
            format!("{} items exceeds {MAX_SETTINGS_WARNINGS}", warnings.len()),
        ));
    }
    for (index, warning) in warnings.iter().enumerate() {
        validate_settings_warning(warning, &format!("{path}[{index}]"))?;
    }
    Ok(())
}

fn validate_settings_warning(value: &Value, path: &str) -> Result<(), ParseError> {
    let warning = value.as_object().ok_or_else(|| {
        ParseError::new(
            ParseErrorKind::MalformedJson,
            path,
            "warning must be an object",
        )
    })?;

    let code = warning.get("code").and_then(Value::as_str).ok_or_else(|| {
        ParseError::new(
            ParseErrorKind::MalformedJson,
            format!("{path}.code"),
            "warning code must be a string",
        )
    })?;
    if !is_settings_warning_code(code) {
        return Err(ParseError::new(
            ParseErrorKind::MalformedJson,
            format!("{path}.code"),
            "warning code is not extension-compatible",
        ));
    }

    validate_optional_limited_text(
        warning.get("field"),
        &format!("{path}.field"),
        MAX_WARNING_FIELD_CHARS,
        "warning field",
    )?;
    validate_optional_limited_text(
        warning.get("message"),
        &format!("{path}.message"),
        MAX_WARNING_MESSAGE_CHARS,
        "warning message",
    )
}

fn validate_optional_limited_text(
    value: Option<&Value>,
    path: &str,
    max_chars: usize,
    label: &str,
) -> Result<(), ParseError> {
    let Some(value) = value else {
        return Ok(());
    };
    let text = value.as_str().ok_or_else(|| {
        ParseError::new(
            ParseErrorKind::MalformedJson,
            path,
            format!("{label} must be a string"),
        )
    })?;
    if text.chars().count() > max_chars {
        return Err(ParseError::new(
            ParseErrorKind::MalformedJson,
            path,
            format!("{label} exceeds {max_chars} characters"),
        ));
    }
    Ok(())
}

fn is_settings_warning_code(code: &str) -> bool {
    matches!(
        code,
        "os-too-old"
            | "encoder-missing"
            | "device-busy"
            | "permission-denied"
            | "policy-denied"
            | "unsupported-browser"
            | "unsupported-profile"
            | "unsupported-platform"
            | "unsupported-display-server"
            | "platform-permission-required"
            | "notarization-invalid"
            | "protected-content"
            | "malformed-message"
            | "unsupported-version"
            | "incompatible-settings"
            | "stale-controller-lease"
            | "duplicate-command"
            | "unsupported-capability"
            | "quota-exceeded"
            | "hash-mismatch"
            | "duplicate-or-replay"
            | "oversized-payload"
            | "storage-failed"
            | "invalid-selection"
            | "user-cancelled"
            | "app-upgrade-required"
            | "extension-upgrade-required"
            | "incompatible-protocol"
            | "repair-required"
            | "disk-pressure"
            | "native-message-too-large"
            | "capture-source-lost"
            | "audio-source-lost"
            | "encoder-failed"
            | "native-finalization-failed"
            | "unknown"
            | "clamped"
            | "deferred-evaluation"
    )
}
