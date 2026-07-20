use serde_json::Value;

use crate::protocol::{
    errors::{ParseError, ParseErrorKind},
    versions::{
        CHUNK_MAX_BASE64_CHARS, MAX_FILENAME_BYTES, MAX_ID_BYTES, MAX_MICROPHONE_DEVICES,
        MAX_TRAY_ACTIONS, MAX_TRAY_SHORTCUT_PRIORITY_LABELS, MAX_UNAVAILABLE_REASONS,
    },
};

pub(super) fn validate_identifier_field(
    key: &str,
    value: &Value,
    path: &str,
) -> Result<(), ParseError> {
    if !(key.ends_with("Id") || matches!(key, "id" | "profileKey")) {
        return Ok(());
    }

    let id = value.as_str().ok_or_else(|| {
        ParseError::new(
            ParseErrorKind::InvalidIdentifier,
            path,
            "identifier must be a string",
        )
    })?;
    validate_identifier(path, id)
}

pub(super) fn validate_filename_field(
    key: &str,
    value: &Value,
    path: &str,
) -> Result<(), ParseError> {
    if key != "filename" {
        return Ok(());
    }

    let filename = value.as_str().ok_or_else(|| {
        ParseError::new(
            ParseErrorKind::InvalidFilename,
            path,
            "filename must be a string",
        )
    })?;
    validate_filename(path, filename)
}

pub(super) fn validate_base64_field(
    key: &str,
    value: &Value,
    path: &str,
) -> Result<(), ParseError> {
    if key != "base64" {
        return Ok(());
    }

    let payload = value.as_str().ok_or_else(|| {
        ParseError::new(
            ParseErrorKind::InvalidBase64,
            path,
            "base64 payload must be a string",
        )
    })?;
    if payload.len() > CHUNK_MAX_BASE64_CHARS {
        return Err(ParseError::new(
            ParseErrorKind::PayloadTooLarge,
            path,
            format!("base64 payload has {} chars", payload.len()),
        ));
    }
    validate_base64_payload(path, payload)
}

pub(super) fn validate_hash_field(key: &str, value: &Value, path: &str) -> Result<(), ParseError> {
    if key != "sha256" && !key.ends_with("Sha256") {
        return Ok(());
    }

    let hash = value.as_str().ok_or_else(|| {
        ParseError::new(
            ParseErrorKind::InvalidHash,
            path,
            "SHA-256 field must be a string",
        )
    })?;

    if hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(ParseError::new(
            ParseErrorKind::InvalidHash,
            path,
            "SHA-256 field must be 64 hex characters",
        ));
    }

    Ok(())
}

pub(super) fn validate_digit_string_field(
    key: &str,
    value: &Value,
    path: &str,
) -> Result<(), ParseError> {
    if key != "startedAtMonotonicNs" {
        return Ok(());
    }

    let text = value.as_str().ok_or_else(|| {
        ParseError::new(
            ParseErrorKind::InvalidNumber,
            path,
            "monotonic timestamp must be a string",
        )
    })?;
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(ParseError::new(
            ParseErrorKind::InvalidNumber,
            path,
            "monotonic timestamp must contain only decimal digits",
        ));
    }

    Ok(())
}

pub(super) fn validate_array_limit(key: &str, value: &Value, path: &str) -> Result<(), ParseError> {
    let Some(items) = value.as_array() else {
        return Ok(());
    };

    let limit = match key {
        "microphoneDevices" => Some(MAX_MICROPHONE_DEVICES),
        "actions" => Some(MAX_TRAY_ACTIONS),
        "shortcutLabels" => Some(MAX_TRAY_SHORTCUT_PRIORITY_LABELS),
        "unavailableReasons" => Some(MAX_UNAVAILABLE_REASONS),
        _ => None,
    };

    if let Some(limit) = limit
        && items.len() > limit
    {
        return Err(ParseError::new(
            ParseErrorKind::ArrayTooLong,
            path,
            format!("{} items exceeds {limit}", items.len()),
        ));
    }

    Ok(())
}

fn validate_identifier(path: &str, value: &str) -> Result<(), ParseError> {
    if value.is_empty() || value.len() > MAX_ID_BYTES {
        return Err(ParseError::new(
            ParseErrorKind::InvalidIdentifier,
            path,
            "identifier length is out of bounds",
        ));
    }

    if !value.bytes().all(is_ascii_id_byte) {
        return Err(ParseError::new(
            ParseErrorKind::InvalidIdentifier,
            path,
            "identifier must use extension-compatible ASCII id characters",
        ));
    }

    Ok(())
}

fn is_ascii_id_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-')
}

fn validate_base64_payload(path: &str, value: &str) -> Result<(), ParseError> {
    let padding_start = value.find('=').unwrap_or(value.len());
    let padding_len = value.len() - padding_start;

    if padding_len > 2
        || !value[padding_start..].bytes().all(|byte| byte == b'=')
        || !value[..padding_start]
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/'))
    {
        return Err(ParseError::new(
            ParseErrorKind::InvalidBase64,
            path,
            "base64 payload must use extension-compatible alphabet and trailing padding",
        ));
    }

    Ok(())
}

fn validate_filename(path: &str, value: &str) -> Result<(), ParseError> {
    let lower = value.to_ascii_lowercase();
    let reserved = [
        "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
        "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
    ];
    let stem = lower.split('.').next().unwrap_or_default();

    if value.is_empty()
        || matches!(value, "." | "..")
        || value.len() > MAX_FILENAME_BYTES
        || value.contains('/')
        || value.contains('\\')
        || value.contains(':')
        || value.bytes().any(|byte| byte.is_ascii_control())
        || reserved.contains(&stem)
    {
        return Err(ParseError::new(
            ParseErrorKind::InvalidFilename,
            path,
            "filename must be a storage leaf name",
        ));
    }

    Ok(())
}
