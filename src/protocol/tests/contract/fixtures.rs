use super::*;

#[test]
fn positive_wire_contract_fixtures_parse() -> Result<(), Box<dyn Error>> {
    for (contract_id, value) in load_fixtures("positive")? {
        if is_wire_message_contract(&contract_id) {
            let message = parse_wire_message(&to_bytes(&value)?)?;
            let roundtrip = serde_json::to_value(message)?;
            assert_eq!(roundtrip, value, "roundtrip drift for {contract_id}");
        }
    }
    Ok(())
}

#[test]
fn telemetry_fixture_exercises_extension_project_shape() -> Result<(), Box<dyn Error>> {
    let fixture = positive_fixture("NativeRecordingTelemetrySnapshot")?;
    let action = fixture["actionEvents"]
        .as_array()
        .and_then(|items| items.first())
        .ok_or("NativeRecordingTelemetrySnapshot actionEvents must be non-empty")?;
    assert_eq!(action["id"], "native-key-0");
    assert_eq!(action["kind"], "KEY");
    assert!(action["time"].is_number());
    assert!(action["duration"].is_number());
    assert!(action["point"].is_null());
    assert_eq!(action["label"], "Ctrl+S");
    assert!(action["data"].is_object());
    assert_eq!(action["preset"], "NONE");

    let signal = fixture["signals"]
        .as_array()
        .and_then(|items| items.first())
        .ok_or("NativeRecordingTelemetrySnapshot signals must be non-empty")?;
    assert_eq!(signal["id"], "native-typing-0");
    assert_eq!(signal["kind"], "typing");
    assert!(signal["startTime"].is_number());
    assert!(signal["endTime"].is_number());
    assert!(signal["point"].is_null());
    assert_eq!(signal["data"]["count"], 3);

    let stopped = positive_fixture("AppRecordingStoppedMessage")?;
    assert_eq!(stopped["telemetry"], fixture);
    Ok(())
}

#[test]
fn negative_wire_contract_fixtures_are_rejected() -> Result<(), Box<dyn Error>> {
    for (contract_id, value) in load_fixtures("negative")? {
        if is_wire_message_contract(&contract_id) {
            let result = parse_wire_message(&to_bytes(&value)?);
            assert!(result.is_err(), "negative fixture parsed for {contract_id}");
        }
    }
    Ok(())
}

#[test]
fn malformed_json_is_rejected() {
    let error = parse_wire_message(br#"{"type":"extension.hello""#);
    assert!(matches!(
        error.as_ref().map_err(ParseError::kind),
        Err(ParseErrorKind::MalformedJson)
    ));
}

#[test]
fn non_object_json_is_rejected() {
    let error = parse_wire_message(br"[]");
    assert!(matches!(
        error.as_ref().map_err(ParseError::kind),
        Err(ParseErrorKind::MalformedJson)
    ));
}

#[test]
fn missing_type_is_rejected() {
    let error = parse_wire_message(br#"{"protocolVersion":1}"#);
    assert!(matches!(
        error.as_ref().map_err(ParseError::kind),
        Err(ParseErrorKind::MissingType)
    ));
}

#[test]
fn oversized_message_is_rejected_before_json_parse() {
    let error = parse_wire_message_with_limit(br#"{"unterminated":"#, 4);
    assert!(matches!(
        error.as_ref().map_err(ParseError::kind),
        Err(ParseErrorKind::OversizedMessage)
    ));
}

#[test]
fn unsupported_protocol_version_is_rejected() -> Result<(), Box<dyn Error>> {
    let mut payload = positive_fixture("ExtensionHelloMessage")?;
    payload["protocolVersion"] = json!(PROTOCOL_VERSION + 1);
    let error = parse_wire_message(&to_bytes(&payload)?);
    assert!(matches!(
        error.as_ref().map_err(ParseError::kind),
        Err(ParseErrorKind::UnsupportedVersion)
    ));
    Ok(())
}
