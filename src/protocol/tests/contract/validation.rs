use super::*;
use crate::protocol::WireMessage;
use proptest::{prelude::*, test_runner::TestCaseError};

fn valid_identifier_byte() -> impl Strategy<Value = u8> {
    prop_oneof![
        b'a'..=b'z',
        b'A'..=b'Z',
        b'0'..=b'9',
        Just(b'.'),
        Just(b'_'),
        Just(b':'),
        Just(b'-'),
    ]
}

fn valid_identifier_fragment() -> impl Strategy<Value = String> {
    prop::collection::vec(valid_identifier_byte(), 0..64)
        .prop_map(|bytes| bytes.into_iter().map(char::from).collect())
}

fn identifier_with_disallowed_ascii_byte() -> impl Strategy<Value = String> {
    (
        valid_identifier_fragment(),
        prop_oneof![
            Just('/'),
            Just('\\'),
            Just(' '),
            Just('\n'),
            Just('\0'),
            Just('@'),
        ],
        valid_identifier_fragment(),
    )
        .prop_map(|(prefix, bad, suffix)| format!("{prefix}{bad}{suffix}"))
}

fn parse_ack_with_controller_lease_id(
    controller_lease_id: String,
) -> Result<Result<WireMessage, ParseError>, TestCaseError> {
    let mut payload = positive_fixture("ExtensionScreenshotAckMessage")
        .map_err(|error| TestCaseError::fail(error.to_string()))?;
    payload["controllerLeaseId"] = Value::String(controller_lease_id);
    let bytes = to_bytes(&payload).map_err(|error| TestCaseError::fail(error.to_string()))?;
    Ok(parse_wire_message(&bytes))
}

proptest! {
    #[test]
    fn controller_lease_identifier_rejects_extension_incompatible_ascii(
        controller_lease_id in identifier_with_disallowed_ascii_byte()
    ) {
        let error = parse_ack_with_controller_lease_id(controller_lease_id)?;

        prop_assert!(matches!(
            error.as_ref().map_err(ParseError::kind),
            Err(ParseErrorKind::InvalidIdentifier)
        ));
    }
}

#[test]
fn missing_controller_lease_is_rejected() {
    let payload = br#"{
        "type": "extension.screenshot.ack",
        "protocolVersion": 1,
        "captureId": "capture-1",
        "assetId": "asset-1"
    }"#;
    let error = parse_wire_message(payload);
    assert!(matches!(
        error.as_ref().map_err(ParseError::kind),
        Err(ParseErrorKind::MalformedJson)
    ));
}

#[test]
fn invalid_identifier_is_rejected() {
    let payload = br#"{
        "type": "extension.screenshot.ack",
        "protocolVersion": 1,
        "controllerLeaseId": "../lease",
        "captureId": "capture-1",
        "assetId": "asset-1"
    }"#;
    let error = parse_wire_message(payload);
    assert!(matches!(
        error.as_ref().map_err(ParseError::kind),
        Err(ParseErrorKind::InvalidIdentifier)
    ));
}

#[test]
fn non_string_identifier_is_rejected() {
    let payload = br#"{
        "type": "extension.screenshot.ack",
        "protocolVersion": 1,
        "controllerLeaseId": 42,
        "captureId": "capture-1",
        "assetId": "asset-1"
    }"#;
    let error = parse_wire_message(payload);
    assert!(matches!(
        error.as_ref().map_err(ParseError::kind),
        Err(ParseErrorKind::InvalidIdentifier)
    ));
}

#[test]
fn overly_long_identifier_is_rejected() -> Result<(), Box<dyn Error>> {
    let mut payload = positive_fixture("ExtensionScreenshotAckMessage")?;
    payload["controllerLeaseId"] = Value::String("a".repeat(129));
    let error = parse_wire_message(&to_bytes(&payload)?);
    assert!(matches!(
        error.as_ref().map_err(ParseError::kind),
        Err(ParseErrorKind::InvalidIdentifier)
    ));
    Ok(())
}

#[test]
fn extension_incompatible_identifier_characters_are_rejected() -> Result<(), Box<dyn Error>> {
    let mut payload = positive_fixture("ExtensionScreenshotAckMessage")?;
    payload["controllerLeaseId"] = Value::String("lease 1".to_owned());
    let error = parse_wire_message(&to_bytes(&payload)?);
    assert!(matches!(
        error.as_ref().map_err(ParseError::kind),
        Err(ParseErrorKind::InvalidIdentifier)
    ));
    Ok(())
}

#[test]
fn controller_acquire_requires_browser_family() -> Result<(), Box<dyn Error>> {
    let mut payload = positive_fixture("ExtensionControllerAcquireMessage")?;
    payload
        .as_object_mut()
        .ok_or("fixture must be an object")?
        .remove("browserFamily");

    let error = parse_wire_message(&to_bytes(&payload)?);

    assert!(matches!(
        error.as_ref().map_err(ParseError::kind),
        Err(ParseErrorKind::MalformedJson)
    ));
    Ok(())
}

#[test]
fn controller_acquire_requires_profile_key() -> Result<(), Box<dyn Error>> {
    let mut payload = positive_fixture("ExtensionControllerAcquireMessage")?;
    payload
        .as_object_mut()
        .ok_or("fixture must be an object")?
        .remove("profileKey");

    let error = parse_wire_message(&to_bytes(&payload)?);

    assert!(matches!(
        error.as_ref().map_err(ParseError::kind),
        Err(ParseErrorKind::MalformedJson)
    ));
    Ok(())
}

#[test]
fn controller_acquire_rejects_invalid_profile_key() -> Result<(), Box<dyn Error>> {
    let mut payload = positive_fixture("ExtensionControllerAcquireMessage")?;
    payload["profileKey"] = Value::String("profile/../1".to_owned());

    let error = parse_wire_message(&to_bytes(&payload)?);

    assert!(matches!(
        error.as_ref().map_err(ParseError::kind),
        Err(ParseErrorKind::InvalidIdentifier)
    ));
    Ok(())
}

#[test]
fn controller_acquire_rejects_oversize_profile_key() -> Result<(), Box<dyn Error>> {
    let mut payload = positive_fixture("ExtensionControllerAcquireMessage")?;
    payload["profileKey"] = Value::String("a".repeat(129));

    let error = parse_wire_message(&to_bytes(&payload)?);

    assert!(matches!(
        error.as_ref().map_err(ParseError::kind),
        Err(ParseErrorKind::InvalidIdentifier)
    ));
    Ok(())
}

#[test]
fn controller_acquire_rejects_invalid_document_id() -> Result<(), Box<dyn Error>> {
    let mut payload = positive_fixture("ExtensionControllerAcquireMessage")?;
    payload["documentId"] = Value::String("document 1".to_owned());

    let error = parse_wire_message(&to_bytes(&payload)?);

    assert!(matches!(
        error.as_ref().map_err(ParseError::kind),
        Err(ParseErrorKind::InvalidIdentifier)
    ));
    Ok(())
}

#[test]
fn controller_acquire_accepts_omitted_document_id() -> Result<(), Box<dyn Error>> {
    let mut payload = positive_fixture("ExtensionControllerAcquireMessage")?;
    payload
        .as_object_mut()
        .ok_or("fixture must be an object")?
        .remove("documentId");

    let message = parse_wire_message(&to_bytes(&payload)?)?;

    let WireMessage::ExtensionControllerAcquire(acquire) = message else {
        return Err("expected controller acquire message".into());
    };
    assert!(acquire.document_id.is_none());
    Ok(())
}

#[test]
fn controller_acquire_rejects_unknown_browser_family() -> Result<(), Box<dyn Error>> {
    let mut payload = positive_fixture("ExtensionControllerAcquireMessage")?;
    payload["browserFamily"] = Value::String("firefox".to_owned());

    let error = parse_wire_message(&to_bytes(&payload)?);

    assert!(matches!(
        error.as_ref().map_err(ParseError::kind),
        Err(ParseErrorKind::UnknownType)
    ));
    Ok(())
}

#[test]
fn recording_start_source_requires_mode_field() -> Result<(), Box<dyn Error>> {
    let mut payload = positive_fixture("ExtensionRecordingStartCommandMessage")?;
    payload["source"] = json!({ "kind": "screen" });

    let error = parse_wire_message(&to_bytes(&payload)?);

    assert!(matches!(
        error.as_ref().map_err(ParseError::kind),
        Err(ParseErrorKind::MalformedJson)
    ));
    Ok(())
}

#[test]
fn recording_start_requested_quality_rejects_legacy_preset() -> Result<(), Box<dyn Error>> {
    let mut payload = positive_fixture("ExtensionRecordingStartCommandMessage")?;
    payload["requestedQuality"] = json!({ "preset": "high" });

    let error = parse_wire_message(&to_bytes(&payload)?);

    assert!(matches!(
        error.as_ref().map_err(ParseError::kind),
        Err(ParseErrorKind::MalformedJson)
    ));
    Ok(())
}

#[test]
fn recording_started_effective_quality_requires_effective_fields() -> Result<(), Box<dyn Error>> {
    let mut payload = positive_fixture("AppRecordingStartedMessage")?;
    payload["effectiveQuality"] = json!({ "fps": 30 });

    let error = parse_wire_message(&to_bytes(&payload)?);

    assert!(matches!(
        error.as_ref().map_err(ParseError::kind),
        Err(ParseErrorKind::MalformedJson)
    ));
    Ok(())
}

#[test]
fn settings_sync_tray_shortcut_priority_rejects_unknown_policy() -> Result<(), Box<dyn Error>> {
    let mut payload = positive_fixture("ExtensionSettingsSyncMessage")?;
    payload["settings"]["trayActions"]["shortcutPriority"]["winner"] = json!("native");

    let error = parse_wire_message(&to_bytes(&payload)?);

    assert!(matches!(
        error.as_ref().map_err(ParseError::kind),
        Err(ParseErrorKind::UnknownType)
    ));
    Ok(())
}

#[test]
fn settings_sync_tray_shortcut_priority_labels_are_bounded() -> Result<(), Box<dyn Error>> {
    let mut payload = positive_fixture("ExtensionSettingsSyncMessage")?;
    payload["settings"]["trayActions"]["shortcutPriority"]["shortcutLabels"] =
        json!(vec!["Ctrl+Shift+S"; 65]);

    let error = parse_wire_message(&to_bytes(&payload)?);

    assert!(matches!(
        error.as_ref().map_err(ParseError::kind),
        Err(ParseErrorKind::ArrayTooLong)
    ));
    Ok(())
}

#[test]
fn settings_accepted_effective_settings_requires_settings_shape() -> Result<(), Box<dyn Error>> {
    let mut payload = positive_fixture("AppSettingsAcceptedMessage")?;
    payload["effectiveSettings"] = json!({});

    let error = parse_wire_message(&to_bytes(&payload)?);

    assert!(matches!(
        error.as_ref().map_err(ParseError::kind),
        Err(ParseErrorKind::MalformedJson)
    ));
    Ok(())
}

#[test]
fn settings_warning_requires_code_field() -> Result<(), Box<dyn Error>> {
    let mut payload = positive_fixture("AppSettingsAcceptedMessage")?;
    payload["warnings"] = json!([{}]);

    let error = parse_wire_message(&to_bytes(&payload)?);

    assert!(matches!(
        error.as_ref().map_err(ParseError::kind),
        Err(ParseErrorKind::MalformedJson)
    ));
    Ok(())
}

#[test]
fn settings_warning_rejects_unknown_code() -> Result<(), Box<dyn Error>> {
    let mut payload = positive_fixture("AppSettingsAcceptedMessage")?;
    payload["warnings"] = json!([{ "code": "unknown-code" }]);

    let error = parse_wire_message(&to_bytes(&payload)?);

    assert!(matches!(
        error.as_ref().map_err(ParseError::kind),
        Err(ParseErrorKind::MalformedJson)
    ));
    Ok(())
}

#[test]
fn settings_warning_rejects_overlong_field() -> Result<(), Box<dyn Error>> {
    let mut payload = positive_fixture("AppSettingsAcceptedMessage")?;
    payload["warnings"] = json!([{ "code": "clamped", "field": "x".repeat(81) }]);

    let error = parse_wire_message(&to_bytes(&payload)?);

    assert!(matches!(
        error.as_ref().map_err(ParseError::kind),
        Err(ParseErrorKind::MalformedJson)
    ));
    Ok(())
}

#[test]
fn settings_warning_rejects_overlong_array() -> Result<(), Box<dyn Error>> {
    let mut payload = positive_fixture("AppSettingsAcceptedMessage")?;
    payload["warnings"] = json!(
        (0..33)
            .map(|_| json!({ "code": "clamped" }))
            .collect::<Vec<_>>()
    );

    let error = parse_wire_message(&to_bytes(&payload)?);

    assert!(matches!(
        error.as_ref().map_err(ParseError::kind),
        Err(ParseErrorKind::ArrayTooLong)
    ));
    Ok(())
}
