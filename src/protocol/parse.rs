//! Untrusted JSON narrowing into typed protocol commands and events.

use serde_json::Value;

use super::{
    errors::{ParseError, ParseErrorKind},
    validate::validate_message_value,
    versions::EXTENSION_TO_HOST_MAX_MESSAGE_BYTES,
    wire::WireMessage,
};

#[must_use]
#[cfg(test)]
pub(crate) fn is_wire_message_contract(contract_id: &str) -> bool {
    matches!(
        contract_id,
        "ExtensionHelloMessage"
            | "AppHelloMessage"
            | "ExtensionControllerAcquireMessage"
            | "AppControllerLeaseMessage"
            | "ExtensionSettingsSyncMessage"
            | "AppSettingsAcceptedMessage"
            | "ExtensionPingMessage"
            | "AppPongMessage"
            | "AppConnectionClosedMessage"
            | "AppTrayActionRequestedMessage"
            | "ExtensionTrayActionResultMessage"
            | "AppOpenSettingsRequestedMessage"
            | "ExtensionScreenshotCaptureCommandMessage"
            | "ExtensionRecordingStartCommandMessage"
            | "ExtensionRecordingControlCommandMessage"
            | "AppCommandAcceptedMessage"
            | "AppScreenshotStartMessage"
            | "AppScreenshotChunkMessage"
            | "AppScreenshotCommitMessage"
            | "ExtensionScreenshotChunkRequestMessage"
            | "ExtensionScreenshotAckMessage"
            | "ExtensionScreenshotRejectMessage"
            | "AppRecordingStartedMessage"
            | "AppRecordingProgressMessage"
            | "AppRecordingStoppedMessage"
            | "ExtensionRecordingChunkRequestMessage"
            | "AppRecordingChunkMessage"
            | "ExtensionRecordingAckMessage"
            | "ExtensionRecordingRejectMessage"
            | "AppOperationFailedMessage"
            | "ExtensionRecordingAttachmentManifestRequestMessage"
            | "AppRecordingAttachmentManifestMessage"
            | "ExtensionRecordingAttachmentChunkRequestMessage"
            | "AppRecordingAttachmentChunkMessage"
            | "ExtensionRecordingAttachmentAckMessage"
            | "AppRecordingAttachmentAvailableMessage"
            | "ExtensionRecordingEventsManifestRequestMessage"
            | "AppRecordingEventsManifestMessage"
            | "ExtensionRecordingEventsChunkRequestMessage"
            | "AppRecordingEventsChunkMessage"
            | "ExtensionRecordingEventsAckMessage"
            | "AppRecordingEventsAvailableMessage"
    )
}

pub(crate) fn parse_wire_message(bytes: &[u8]) -> Result<WireMessage, ParseError> {
    parse_wire_message_with_limit(bytes, EXTENSION_TO_HOST_MAX_MESSAGE_BYTES)
}

pub(crate) fn parse_wire_message_with_limit(
    bytes: &[u8],
    max_message_bytes: usize,
) -> Result<WireMessage, ParseError> {
    if bytes.len() > max_message_bytes {
        return Err(ParseError::new(
            ParseErrorKind::OversizedMessage,
            "message",
            format!("{} bytes exceeds {max_message_bytes}", bytes.len()),
        ));
    }

    let value = parse_json_object(bytes)?;
    let message_type = message_type(&value)?;
    validate_message_value(&value, message_type, bytes.len())?;
    if matches!(
        message_type,
        "app.recording.attachment.manifest" | "app.recording.events.manifest"
    ) && value.get("nextSequence").is_none()
    {
        return Err(ParseError::without_field(
            ParseErrorKind::MalformedJson,
            "attachment page requires nextSequence",
        ));
    }
    let message = serde_json::from_slice(bytes).map_err(|error| {
        let detail = error.to_string();
        if detail.contains("unknown variant") {
            ParseError::new(ParseErrorKind::UnknownType, "type", detail)
        } else {
            ParseError::without_field(ParseErrorKind::MalformedJson, detail)
        }
    })?;
    super::trajectory::validate_attachment_message(&message).map_err(|_| {
        ParseError::without_field(
            ParseErrorKind::MalformedJson,
            "invalid cursor attachment contract",
        )
    })?;
    super::recording_events::validate_event_attachment_message(&message).map_err(|_| {
        ParseError::without_field(
            ParseErrorKind::MalformedJson,
            "invalid recording events contract",
        )
    })?;
    Ok(message)
}

fn parse_json_object(bytes: &[u8]) -> Result<Value, ParseError> {
    let value = serde_json::from_slice::<Value>(bytes).map_err(|error| {
        ParseError::without_field(ParseErrorKind::MalformedJson, error.to_string())
    })?;
    if value.is_object() {
        Ok(value)
    } else {
        Err(ParseError::without_field(
            ParseErrorKind::MalformedJson,
            "wire payload must be a JSON object",
        ))
    }
}

fn message_type(value: &Value) -> Result<&str, ParseError> {
    value
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| ParseError::new(ParseErrorKind::MissingType, "type", "missing message type"))
}
