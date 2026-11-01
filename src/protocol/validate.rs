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
    validate_desktop_contract(value, message_type)?;
    validate_recursive(value, "$", message_type)?;
    validate_transfer_bounds(value, message_type, raw_len)?;
    Ok(())
}

fn validate_desktop_contract(value: &Value, message_type: &str) -> Result<(), ParseError> {
    let valid = match message_type {
        "app.hello" => {
            let id = value["appIdentity"]["clientId"]
                .as_str()
                .unwrap_or_default();
            id.len() == 64
                && id
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                && value["appIdentity"]["trustVersion"] == 1
                && value["settingsAuthority"] == "desktop"
        }
        "extension.hello" => value["capabilities"].as_array().is_some_and(|items| {
            items.len() <= 16
                && items
                    .iter()
                    .all(|item| item.as_str().is_some_and(|text| text.len() <= 64))
                && ["desktop-settings-v1", "durable-media-ack-v1"]
                    .iter()
                    .all(|name| items.iter().any(|item| item == name))
        }),
        "app.controller.lease" => match value["selectionStatus"].as_str() {
            Some("selected") => value["status"] == "granted",
            Some("available" | "selection-required") => value["status"] != "granted",
            _ => false,
        },
        "extension.screenshot.ack" | "extension.recording.ack" => {
            value["receipt"]["durable"] == true
                && value["receipt"]["totalBytes"]
                    .as_u64()
                    .is_some_and(|size| size > 0)
        }
        _ => true,
    };
    if valid {
        Ok(())
    } else {
        Err(ParseError::new(
            ParseErrorKind::MalformedJson,
            "desktop-contract",
            "Desktop contract requirements are missing or invalid",
        ))
    }
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

fn validate_recursive(value: &Value, path: &str, message_type: &str) -> Result<(), ParseError> {
    // These closed typed shapes use trajectory counters, including valid empty prefixes.
    // Their owner validates them after deserialization; video totalBytes stays positive.
    if (matches!(path, "$.cursorTrajectory" | "$.recordingEvents")
        && message_type == "app.recording.stopped")
        || (path == "$.descriptor"
            && matches!(
                message_type,
                "app.recording.attachment.manifest"
                    | "app.recording.attachment.available"
                    | "app.recording.events.manifest"
                    | "app.recording.events.available"
            ))
        || (path == "$.receipt"
            && matches!(
                message_type,
                "extension.recording.attachment.ack" | "extension.recording.events.ack"
            ))
    {
        return Ok(());
    }
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                let child_path = format!("{path}.{key}");
                validate_named_value(key, child, &child_path)?;
                validate_recursive(child, &child_path, message_type)?;
            }
            Ok(())
        }
        Value::Array(items) => {
            for (index, child) in items.iter().enumerate() {
                validate_recursive(child, &format!("{path}[{index}]"), message_type)?;
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
    if matches!(
        message_type,
        "app.screenshot.chunk" | "app.recording.chunk" | "app.recording.attachment.chunk"
    ) && raw_len > MESSAGE_TARGET_MAX_JSON_UTF8_BYTES.min(HOST_TO_EXTENSION_MAX_MESSAGE_BYTES)
    {
        return Err(ParseError::new(
            ParseErrorKind::PayloadTooLarge,
            "message",
            format!("chunk message has {raw_len} UTF-8 bytes"),
        ));
    }

    Ok(())
}
