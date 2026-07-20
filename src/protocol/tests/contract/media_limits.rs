use super::*;

#[test]
fn invalid_filename_is_rejected() {
    let payload = br#"{
        "type": "app.screenshot.start",
        "protocolVersion": 1,
        "controllerLeaseId": "lease-1",
        "captureId": "capture-1",
        "mode": "screen",
        "filename": "C:\\secret.png",
        "mimeType": "image/png",
        "totalBytes": 1,
        "width": 1,
        "height": 1,
        "chunkCount": 1,
        "sha256": "abcdef",
        "openEditor": true,
        "capturedAtEpochMs": 1
    }"#;
    let error = parse_wire_message(payload);
    assert!(matches!(
        error.as_ref().map_err(ParseError::kind),
        Err(ParseErrorKind::InvalidFilename)
    ));
}

#[test]
fn non_string_filename_is_rejected() -> Result<(), Box<dyn Error>> {
    let mut payload = positive_fixture("AppScreenshotStartMessage")?;
    payload["filename"] = json!(42);
    let error = parse_wire_message(&to_bytes(&payload)?);
    assert!(matches!(
        error.as_ref().map_err(ParseError::kind),
        Err(ParseErrorKind::InvalidFilename)
    ));
    Ok(())
}

#[test]
fn dot_filename_is_rejected() -> Result<(), Box<dyn Error>> {
    for filename in [".", ".."] {
        let mut payload = positive_fixture("AppScreenshotStartMessage")?;
        payload["filename"] = json!(filename);
        let error = parse_wire_message(&to_bytes(&payload)?);
        assert!(matches!(
            error.as_ref().map_err(ParseError::kind),
            Err(ParseErrorKind::InvalidFilename)
        ));
    }
    Ok(())
}

#[test]
fn non_string_base64_is_rejected() -> Result<(), Box<dyn Error>> {
    let mut payload = positive_fixture("AppScreenshotChunkMessage")?;
    payload["base64"] = json!(42);
    let error = parse_wire_message(&to_bytes(&payload)?);
    assert!(matches!(
        error.as_ref().map_err(ParseError::kind),
        Err(ParseErrorKind::InvalidBase64)
    ));
    Ok(())
}

#[test]
fn invalid_base64_alphabet_is_rejected() -> Result<(), Box<dyn Error>> {
    let mut payload = positive_fixture("AppScreenshotChunkMessage")?;
    payload["base64"] = Value::String("abc-_".to_owned());
    let error = parse_wire_message(&to_bytes(&payload)?);
    assert!(matches!(
        error.as_ref().map_err(ParseError::kind),
        Err(ParseErrorKind::InvalidBase64)
    ));
    Ok(())
}

#[test]
fn oversized_base64_is_rejected() -> Result<(), Box<dyn Error>> {
    let mut payload = positive_fixture("AppScreenshotChunkMessage")?;
    payload["base64"] = Value::String("A".repeat(CHUNK_MAX_BASE64_CHARS + 1));
    let error = parse_wire_message(&to_bytes(&payload)?);
    assert!(matches!(
        error.as_ref().map_err(ParseError::kind),
        Err(ParseErrorKind::PayloadTooLarge)
    ));
    Ok(())
}

#[test]
fn invalid_sha256_is_rejected() -> Result<(), Box<dyn Error>> {
    let mut payload = positive_fixture("AppScreenshotStartMessage")?;
    payload["sha256"] = Value::String("not-a-sha".to_owned());
    let error = parse_wire_message(&to_bytes(&payload)?);
    assert!(matches!(
        error.as_ref().map_err(ParseError::kind),
        Err(ParseErrorKind::InvalidHash)
    ));
    Ok(())
}

#[test]
fn non_integer_chunk_raw_bytes_is_rejected() -> Result<(), Box<dyn Error>> {
    let mut payload = positive_fixture("AppScreenshotChunkMessage")?;
    payload["chunkRawBytes"] = json!("11");
    let error = parse_wire_message(&to_bytes(&payload)?);
    assert!(matches!(
        error.as_ref().map_err(ParseError::kind),
        Err(ParseErrorKind::InvalidNumber)
    ));
    Ok(())
}

#[test]
fn positive_numeric_fields_reject_zero() -> Result<(), Box<dyn Error>> {
    let mut payload = positive_fixture("AppScreenshotStartMessage")?;
    payload["totalBytes"] = json!(0);
    let error = parse_wire_message(&to_bytes(&payload)?);
    assert!(matches!(
        error.as_ref().map_err(ParseError::kind),
        Err(ParseErrorKind::InvalidNumber)
    ));
    Ok(())
}

#[test]
fn array_caps_are_rejected() -> Result<(), Box<dyn Error>> {
    let mut payload = positive_fixture("AppHelloMessage")?;
    payload["capabilities"]["audio"]["microphoneDevices"] = Value::Array(vec![
        json!({
            "id": "mic",
            "label": "Microphone",
            "isDefault": false,
            "available": true
        });
        33
    ]);
    let error = parse_wire_message(&to_bytes(&payload)?);
    assert!(matches!(
        error.as_ref().map_err(ParseError::kind),
        Err(ParseErrorKind::ArrayTooLong)
    ));
    Ok(())
}

#[test]
fn total_size_caps_are_rejected() -> Result<(), Box<dyn Error>> {
    let mut screenshot = positive_fixture("AppScreenshotStartMessage")?;
    screenshot["totalBytes"] = json!(SCREENSHOT_MAX_TOTAL_BYTES + 1);
    let screenshot_error = parse_wire_message(&to_bytes(&screenshot)?);
    assert!(matches!(
        screenshot_error.as_ref().map_err(ParseError::kind),
        Err(ParseErrorKind::PayloadTooLarge)
    ));

    let mut recording = positive_fixture("AppRecordingStoppedMessage")?;
    recording["totalBytes"] = json!(RECORDING_MAX_TOTAL_BYTES + 1);
    let recording_error = parse_wire_message(&to_bytes(&recording)?);
    assert!(matches!(
        recording_error.as_ref().map_err(ParseError::kind),
        Err(ParseErrorKind::PayloadTooLarge)
    ));
    Ok(())
}

#[test]
fn chunk_json_size_cap_is_rejected() -> Result<(), Box<dyn Error>> {
    let mut payload = positive_fixture("AppScreenshotChunkMessage")?;
    payload["padding"] = Value::String("A".repeat(MESSAGE_TARGET_MAX_JSON_UTF8_BYTES));
    let error = parse_wire_message(&to_bytes(&payload)?);
    assert!(matches!(
        error.as_ref().map_err(ParseError::kind),
        Err(ParseErrorKind::PayloadTooLarge)
    ));
    Ok(())
}
