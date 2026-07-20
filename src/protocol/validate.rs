use serde_json::Value;

use super::{
    errors::{ParseError, ParseErrorKind},
    validation::validate_named_value,
    versions::{
        HOST_TO_EXTENSION_MAX_MESSAGE_BYTES, MESSAGE_TARGET_MAX_JSON_UTF8_BYTES, PROTOCOL_VERSION,
        RECORDING_MAX_TOTAL_BYTES, SCREENSHOT_MAX_TOTAL_BYTES,
    },
};

pub(crate) fn validate_message_value(
    value: &Value,
    message_type: &str,
    raw_len: usize,
) -> Result<(), ParseError> {
    validate_protocol_version(value)?;
    validate_recursive(value, "$")?;
    validate_transfer_bounds(value, message_type, raw_len)?;
    Ok(())
}

fn validate_protocol_version(value: &Value) -> Result<(), ParseError> {
    let version = value
        .get("protocolVersion")
        .and_then(Value::as_u64)
        .ok_or_else(|| {
            ParseError::new(
                ParseErrorKind::UnsupportedVersion,
                "protocolVersion",
                "missing or non-integer protocol version",
            )
        })?;

    if version == u64::from(PROTOCOL_VERSION) {
        Ok(())
    } else {
        Err(ParseError::new(
            ParseErrorKind::UnsupportedVersion,
            "protocolVersion",
            format!("unsupported protocol version {version}"),
        ))
    }
}

fn validate_recursive(value: &Value, path: &str) -> Result<(), ParseError> {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                let child_path = format!("{path}.{key}");
                validate_named_value(key, child, &child_path)?;
                validate_recursive(child, &child_path)?;
            }
            Ok(())
        }
        Value::Array(items) => {
            for (index, child) in items.iter().enumerate() {
                validate_recursive(child, &format!("{path}[{index}]"))?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn validate_transfer_bounds(
    value: &Value,
    message_type: &str,
    raw_len: usize,
) -> Result<(), ParseError> {
    let Some(total_bytes) = value.get("totalBytes").and_then(Value::as_u64) else {
        return validate_chunk_json_size(message_type, raw_len);
    };

    let limit = if message_type.starts_with("app.screenshot.") {
        SCREENSHOT_MAX_TOTAL_BYTES
    } else if message_type.starts_with("app.recording.") {
        RECORDING_MAX_TOTAL_BYTES
    } else {
        return validate_chunk_json_size(message_type, raw_len);
    };

    if total_bytes > limit {
        return Err(ParseError::new(
            ParseErrorKind::PayloadTooLarge,
            "totalBytes",
            format!("total bytes {total_bytes} exceeds {limit}"),
        ));
    }

    validate_chunk_json_size(message_type, raw_len)
}

fn validate_chunk_json_size(message_type: &str, raw_len: usize) -> Result<(), ParseError> {
    if matches!(message_type, "app.screenshot.chunk" | "app.recording.chunk")
        && raw_len > MESSAGE_TARGET_MAX_JSON_UTF8_BYTES.min(HOST_TO_EXTENSION_MAX_MESSAGE_BYTES)
    {
        return Err(ParseError::new(
            ParseErrorKind::PayloadTooLarge,
            "message",
            format!("chunk message has {raw_len} UTF-8 bytes"),
        ));
    }

    Ok(())
}
