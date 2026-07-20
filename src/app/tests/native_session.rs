use super::*;

#[test]
fn native_host_clean_eof_keeps_stdout_reserved_for_native_messages() {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut stdin = Cursor::new(Vec::<u8>::new());

    let result = run_with_streams(
        args(&[
            "sniptale-native-agent",
            "--native-host",
            ALLOWED_NATIVE_ORIGIN,
        ]),
        &mut stdin,
        &mut stdout,
        &mut stderr,
    );

    assert!(result.is_ok());
    assert!(stdout.is_empty());
    assert!(stderr.is_empty());
}

#[test]
fn native_host_replies_to_extension_hello_with_framed_app_hello() -> Result<(), Box<dyn Error>> {
    let input = framed(
        br#"{
            "type": "extension.hello",
            "protocolVersion": 1,
            "settingsSchemaVersion": 1,
            "extensionVersion": "0.1.0",
            "extensionId": "abcdefghijklmnopqrstuvwxzyabcdef",
            "minAppVersion": "0.1.0",
            "supportedProtocolVersions": [1],
            "supportedSettingsSchemaVersions": [1]
        }"#,
    )?;
    let mut stdin = Cursor::new(input);
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    assert!(
        run_with_streams(
            args(&[
                "sniptale-native-agent",
                "--native-host",
                ALLOWED_NATIVE_ORIGIN,
            ]),
            &mut stdin,
            &mut stdout,
            &mut stderr,
        )
        .is_ok()
    );

    assert!(stderr.is_empty());
    let value = read_framed_json(&stdout)?;
    assert_eq!(value["type"], "app.hello");
    assert_eq!(value["protocolVersion"], 1);
    assert_eq!(value["install"]["installerVersion"], "0.1.0");
    assert_eq!(value["install"]["nativeHostManifestVersion"], "0.1.0");
    assert_eq!(value["install"]["rollbackProtected"], true);
    assert_eq!(
        value["capabilities"]["capture"]["screenshotModes"],
        Value::Array(Vec::new())
    );
    #[cfg(not(windows))]
    assert_eq!(
        value["capabilities"]["codecs"]["unavailableReasons"][0],
        "unsupported-platform"
    );
    assert_eq!(value["capabilities"]["audio"]["supportsMixedAudio"], false);
    Ok(())
}

#[test]
fn native_host_rejects_incompatible_hello_with_typed_failure() -> Result<(), Box<dyn Error>> {
    let input = framed(
        br#"{
            "type": "extension.hello",
            "protocolVersion": 1,
            "settingsSchemaVersion": 1,
            "extensionVersion": "0.1.0",
            "extensionId": "abcdefghijklmnopqrstuvwxzyabcdef",
            "minAppVersion": "0.1.0",
            "supportedProtocolVersions": [2],
            "supportedSettingsSchemaVersions": [1]
        }"#,
    )?;
    let mut stdin = Cursor::new(input);
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    assert!(
        run_with_streams(
            args(&[
                "sniptale-native-agent",
                "--native-host",
                ALLOWED_NATIVE_ORIGIN,
            ]),
            &mut stdin,
            &mut stdout,
            &mut stderr,
        )
        .is_ok()
    );

    assert!(stderr.is_empty());
    let value = read_framed_json(&stdout)?;
    assert_eq!(value["type"], "app.operation.failed");
    assert_eq!(value["operation"], "handshake");
    assert_eq!(value["phase"], "version-negotiation");
    assert_eq!(value["error"]["code"], "incompatible-protocol");
    assert_eq!(value["error"]["recoverable"], false);
    Ok(())
}

#[test]
fn native_host_requires_accepted_hello_before_controller_acquire() -> Result<(), Box<dyn Error>> {
    let input = framed(
        br#"{
            "type": "extension.controller.acquire",
            "protocolVersion": 1,
            "connectionId": "connection-1",
            "browserFamily": "chrome",
            "profileKey": "profile-1",
            "documentId": "document-1",
            "requestedAtEpochMs": 1000,
            "reason": "initial-connect"
        }"#,
    )?;
    let mut stdin = Cursor::new(input);
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    assert!(
        run_with_streams(
            args(&[
                "sniptale-native-agent",
                "--native-host",
                ALLOWED_NATIVE_ORIGIN,
            ]),
            &mut stdin,
            &mut stdout,
            &mut stderr,
        )
        .is_ok()
    );

    let value = read_framed_json(&stdout)?;
    assert!(stderr.is_empty());
    assert_eq!(value["type"], "app.operation.failed");
    assert_eq!(value["operation"], "handshake");
    assert_eq!(value["operationId"], "connection-1");
    assert_eq!(value["error"]["code"], "malformed-message");
    Ok(())
}

#[test]
fn controller_acquire_reports_pending_recordings_after_reconnect() -> Result<(), Box<dyn Error>> {
    let mut session = NativeHostSession::default();
    let hello_message = parse_wire_message(extension_hello())?;
    let hello_value: Value = serde_json::from_slice(extension_hello())?;
    responses_for_message(&mut session, &hello_message, &hello_value);

    let acquire_message = parse_wire_message(extension_controller_acquire())?;
    let acquire_value: Value = serde_json::from_slice(extension_controller_acquire())?;
    let mut effects = PendingRecordingEffects {
        pending: vec![pending_recording_transfer()],
    };

    let responses = responses_for_message_with_effects(
        &mut session,
        &acquire_message,
        &acquire_value,
        &mut effects,
    );

    assert_eq!(responses.len(), 2);
    assert_eq!(responses[0]["type"], "app.controller.lease");
    assert_eq!(responses[1]["type"], "app.recording.stopped");
    assert_eq!(responses[1]["controllerLeaseId"], "lease:connection-1");
    assert_eq!(responses[1]["recordingId"], "recording-1");
    assert_eq!(responses[1]["filename"], "sniptale-recording.mp4");
    assert_eq!(responses[1]["durationMs"], 4_000);
    assert_eq!(
        responses[1]["totalBytes"],
        crate::protocol::CHUNK_RAW_BYTES + 1
    );
    assert_eq!(responses[1]["chunkCount"], 2);
    assert_eq!(responses[1]["telemetry"], Value::Null);

    let parsed = parse_wire_message(&serde_json::to_vec(&responses[1])?)?;
    let WireMessage::AppRecordingStopped(stopped) = parsed else {
        return Err("expected app.recording.stopped".into());
    };
    assert!(stopped.open_editor);
    Ok(())
}

#[test]
fn controller_acquire_reports_pending_screenshots_after_reconnect() -> Result<(), Box<dyn Error>> {
    let mut session = NativeHostSession::default();
    let hello_message = parse_wire_message(extension_hello())?;
    let hello_value: Value = serde_json::from_slice(extension_hello())?;
    responses_for_message(&mut session, &hello_message, &hello_value);

    let acquire_message = parse_wire_message(extension_controller_acquire())?;
    let acquire_value: Value = serde_json::from_slice(extension_controller_acquire())?;
    let mut effects = PendingScreenshotEffects {
        pending: vec![pending_screenshot_transfer()],
    };

    let responses = responses_for_message_with_effects(
        &mut session,
        &acquire_message,
        &acquire_value,
        &mut effects,
    );

    assert_eq!(responses.len(), 2);
    assert_eq!(responses[0]["type"], "app.controller.lease");
    assert_eq!(responses[1]["type"], "app.screenshot.start");
    assert_eq!(responses[1]["controllerLeaseId"], "lease:connection-1");
    assert_eq!(responses[1]["captureId"], "capture-1");
    assert_eq!(responses[1]["filename"], "capture-1.png");
    assert_eq!(
        responses[1]["totalBytes"],
        crate::protocol::CHUNK_RAW_BYTES + 1
    );
    assert_eq!(responses[1]["chunkCount"], 2);

    let parsed = parse_wire_message(&serde_json::to_vec(&responses[1])?)?;
    let WireMessage::AppScreenshotStart(start) = parsed else {
        return Err("expected app.screenshot.start".into());
    };
    assert!(start.open_editor);
    Ok(())
}

#[test]
fn controller_acquire_reports_recovery_failure_when_pending_scan_fails()
-> Result<(), Box<dyn Error>> {
    let mut session = NativeHostSession::default();
    let hello_message = parse_wire_message(extension_hello())?;
    let hello_value: Value = serde_json::from_slice(extension_hello())?;
    responses_for_message(&mut session, &hello_message, &hello_value);

    let acquire_message = parse_wire_message(extension_controller_acquire())?;
    let acquire_value: Value = serde_json::from_slice(extension_controller_acquire())?;
    let mut effects = FailingReconnectEffects;

    let responses = responses_for_message_with_effects(
        &mut session,
        &acquire_message,
        &acquire_value,
        &mut effects,
    );

    assert_eq!(responses.len(), 2);
    assert_eq!(responses[0]["type"], "app.controller.lease");
    assert_eq!(responses[1]["type"], "app.operation.failed");
    assert_eq!(responses[1]["controllerLeaseId"], "lease:connection-1");
    assert_eq!(responses[1]["operation"], "recording");
    assert_eq!(responses[1]["operationId"], "recording-reconnect");
    assert_eq!(responses[1]["phase"], "startup-recovery");
    assert_eq!(responses[1]["error"]["code"], "storage-failed");
    assert_eq!(responses[1]["error"]["recoverable"], true);
    Ok(())
}

#[test]
fn rejected_tray_action_result_shows_tray_notice() -> Result<(), Box<dyn Error>> {
    let mut session = NativeHostSession::default();
    let value: Value = serde_json::from_slice(
        br#"{
            "type": "extension.tray.actionResult",
            "protocolVersion": 1,
            "controllerLeaseId": "lease:connection-1",
            "invocationId": "tray-invocation-1",
            "accepted": false,
            "error": "recording-inactive"
        }"#,
    )?;
    let message = parse_wire_message(&serde_json::to_vec(&value)?)?;
    let mut effects = TrayNoticeEffects::default();

    let responses =
        responses_for_message_with_effects(&mut session, &message, &value, &mut effects);

    assert!(responses.is_empty());
    assert_eq!(effects.notices, vec![TrayNoticeKind::ActionRejected]);
    Ok(())
}

#[test]
fn accepted_tray_action_result_does_not_show_tray_notice() -> Result<(), Box<dyn Error>> {
    let mut session = NativeHostSession::default();
    let value: Value = serde_json::from_slice(
        br#"{
            "type": "extension.tray.actionResult",
            "protocolVersion": 1,
            "controllerLeaseId": "lease:connection-1",
            "invocationId": "tray-invocation-1",
            "accepted": true
        }"#,
    )?;
    let message = parse_wire_message(&serde_json::to_vec(&value)?)?;
    let mut effects = TrayNoticeEffects::default();

    let responses =
        responses_for_message_with_effects(&mut session, &message, &value, &mut effects);

    assert!(responses.is_empty());
    assert!(effects.notices.is_empty());
    Ok(())
}

#[test]
fn native_host_grants_controller_and_accepts_settings_sync() -> Result<(), Box<dyn Error>> {
    let mut input = Vec::new();
    for payload in [
        extension_hello(),
        br#"{
            "type": "extension.controller.acquire",
            "protocolVersion": 1,
            "connectionId": "connection-1",
            "browserFamily": "chrome",
            "profileKey": "profile-1",
            "documentId": "document-1",
            "requestedAtEpochMs": 1000,
            "reason": "initial-connect"
        }"#
        .as_slice(),
        extension_settings_sync(),
        br#"{
            "type": "extension.ping",
            "protocolVersion": 1,
            "nonce": "ping-1",
            "sentAtEpochMs": 3000
        }"#
        .as_slice(),
    ] {
        input.extend(framed(payload)?);
    }
    let mut stdin = Cursor::new(input);
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    assert!(
        run_with_streams(
            args(&[
                "sniptale-native-agent",
                "--native-host",
                ALLOWED_NATIVE_ORIGIN
            ]),
            &mut stdin,
            &mut stdout,
            &mut stderr,
        )
        .is_ok()
    );

    assert!(stderr.is_empty());
    let values = read_framed_json_values(&stdout)?;
    assert_eq!(values.len(), 4);
    assert_eq!(values[0]["type"], "app.hello");
    assert_eq!(values[1]["type"], "app.controller.lease");
    assert_eq!(values[1]["controllerLeaseId"], "lease:connection-1");
    assert_eq!(
        values[1]["controller"]["extensionId"],
        "abcdefghijklmnopqrstuvwxzyabcdef"
    );
    assert_eq!(values[1]["controller"]["browserFamily"], "chrome");
    assert_eq!(values[1]["controller"]["profileKey"], "profile-1");
    assert_eq!(values[1]["controller"]["documentId"], "document-1");
    assert_eq!(values[1]["controller"]["connectionId"], "connection-1");
    assert_eq!(values[2]["type"], "app.settings.accepted");
    assert_eq!(values[2]["controllerLeaseId"], "lease:connection-1");
    assert_eq!(values[2]["revision"], "settings-revision-1");
    assert_eq!(
        values[2]["effectiveSettings"]["warnings"][0]["code"],
        "deferred-evaluation"
    );
    assert_eq!(values[2]["warnings"][0]["field"], "video.effectiveQuality");
    assert_eq!(values[3]["type"], "app.pong");
    assert_eq!(
        values[3]["appStatus"]["settingsRevision"],
        "settings-revision-1"
    );
    Ok(())
}

#[test]
fn native_host_controller_identity_comes_from_acquire_message() -> Result<(), Box<dyn Error>> {
    let mut input = Vec::new();
    for payload in [
        extension_hello(),
        br#"{
            "type": "extension.controller.acquire",
            "protocolVersion": 1,
            "connectionId": "edge-connection-1",
            "browserFamily": "edge",
            "profileKey": "edge-profile",
            "documentId": "edge-document-1",
            "requestedAtEpochMs": 1000,
            "reason": "initial-connect"
        }"#
        .as_slice(),
    ] {
        input.extend(framed(payload)?);
    }
    let mut stdin = Cursor::new(input);
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    assert!(
        run_with_streams(
            args(&[
                "sniptale-native-agent",
                "--native-host",
                ALLOWED_NATIVE_ORIGIN
            ]),
            &mut stdin,
            &mut stdout,
            &mut stderr,
        )
        .is_ok()
    );

    let values = read_framed_json_values(&stdout)?;
    assert!(stderr.is_empty());
    assert_eq!(values[1]["type"], "app.controller.lease");
    assert_eq!(values[1]["status"], "granted");
    assert_eq!(values[1]["controllerLeaseId"], "lease:edge-connection-1");
    assert_eq!(
        values[1]["controller"]["extensionId"],
        "abcdefghijklmnopqrstuvwxzyabcdef"
    );
    assert_eq!(values[1]["controller"]["browserFamily"], "edge");
    assert_eq!(values[1]["controller"]["profileKey"], "edge-profile");
    assert_eq!(values[1]["controller"]["documentId"], "edge-document-1");
    assert_eq!(values[1]["controller"]["connectionId"], "edge-connection-1");
    Ok(())
}

#[test]
fn native_host_second_initial_controller_does_not_receive_active_lease()
-> Result<(), Box<dyn Error>> {
    let mut input = Vec::new();
    for payload in [
        extension_hello(),
        br#"{
            "type": "extension.controller.acquire",
            "protocolVersion": 1,
            "connectionId": "connection-1",
            "browserFamily": "chrome",
            "profileKey": "profile-1",
            "documentId": "document-1",
            "requestedAtEpochMs": 1000,
            "reason": "initial-connect"
        }"#
        .as_slice(),
        br#"{
            "type": "extension.controller.acquire",
            "protocolVersion": 1,
            "connectionId": "connection-2",
            "browserFamily": "chrome",
            "profileKey": "profile-2",
            "documentId": "document-2",
            "requestedAtEpochMs": 2000,
            "reason": "initial-connect"
        }"#
        .as_slice(),
    ] {
        input.extend(framed(payload)?);
    }
    let mut stdin = Cursor::new(input);
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    assert!(
        run_with_streams(
            args(&[
                "sniptale-native-agent",
                "--native-host",
                ALLOWED_NATIVE_ORIGIN
            ]),
            &mut stdin,
            &mut stdout,
            &mut stderr,
        )
        .is_ok()
    );

    let values = read_framed_json_values(&stdout)?;
    assert!(stderr.is_empty());
    assert_eq!(values[1]["status"], "granted");
    assert_eq!(values[1]["controllerLeaseId"], "lease:connection-1");
    assert_eq!(values[2]["status"], "owned-by-other-profile");
    assert_eq!(values[2]["controller"]["browserFamily"], "chrome");
    assert_eq!(values[2]["controller"]["profileKey"], "profile-1");
    assert_eq!(values[2]["controller"]["documentId"], "document-1");
    assert_eq!(values[2]["controller"]["connectionId"], "connection-1");
    assert_eq!(values[2]["controllerLeaseId"], "lease:connection-2");
    Ok(())
}

#[test]
fn native_host_document_id_difference_does_not_bypass_profile_owner() -> Result<(), Box<dyn Error>>
{
    let mut input = Vec::new();
    for payload in [
        extension_hello(),
        br#"{
            "type": "extension.controller.acquire",
            "protocolVersion": 1,
            "connectionId": "connection-1",
            "browserFamily": "chrome",
            "profileKey": "profile-1",
            "documentId": "document-1",
            "requestedAtEpochMs": 1000,
            "reason": "initial-connect"
        }"#
        .as_slice(),
        br#"{
            "type": "extension.controller.acquire",
            "protocolVersion": 1,
            "connectionId": "connection-2",
            "browserFamily": "chrome",
            "profileKey": "profile-1",
            "documentId": "document-2",
            "requestedAtEpochMs": 2000,
            "reason": "initial-connect"
        }"#
        .as_slice(),
    ] {
        input.extend(framed(payload)?);
    }
    let mut stdin = Cursor::new(input);
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    assert!(
        run_with_streams(
            args(&[
                "sniptale-native-agent",
                "--native-host",
                ALLOWED_NATIVE_ORIGIN
            ]),
            &mut stdin,
            &mut stdout,
            &mut stderr,
        )
        .is_ok()
    );

    let values = read_framed_json_values(&stdout)?;
    assert!(stderr.is_empty());
    assert_eq!(values[1]["status"], "granted");
    assert_eq!(values[2]["status"], "owned-by-other-profile");
    assert_eq!(values[2]["controller"]["profileKey"], "profile-1");
    assert_eq!(values[2]["controller"]["documentId"], "document-1");
    assert_eq!(values[2]["controller"]["connectionId"], "connection-1");
    Ok(())
}

#[test]
fn native_host_browser_family_difference_is_distinct_controller() -> Result<(), Box<dyn Error>> {
    let mut input = Vec::new();
    for payload in [
        extension_hello(),
        br#"{
            "type": "extension.controller.acquire",
            "protocolVersion": 1,
            "connectionId": "chrome-connection",
            "browserFamily": "chrome",
            "profileKey": "profile-1",
            "documentId": "document-1",
            "requestedAtEpochMs": 1000,
            "reason": "initial-connect"
        }"#
        .as_slice(),
        br#"{
            "type": "extension.controller.acquire",
            "protocolVersion": 1,
            "connectionId": "edge-connection",
            "browserFamily": "edge",
            "profileKey": "profile-1",
            "documentId": "document-1",
            "requestedAtEpochMs": 2000,
            "reason": "initial-connect"
        }"#
        .as_slice(),
    ] {
        input.extend(framed(payload)?);
    }
    let mut stdin = Cursor::new(input);
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    assert!(
        run_with_streams(
            args(&[
                "sniptale-native-agent",
                "--native-host",
                ALLOWED_NATIVE_ORIGIN
            ]),
            &mut stdin,
            &mut stdout,
            &mut stderr,
        )
        .is_ok()
    );

    let values = read_framed_json_values(&stdout)?;
    assert!(stderr.is_empty());
    assert_eq!(values[1]["status"], "granted");
    assert_eq!(values[2]["status"], "owned-by-other-profile");
    assert_eq!(values[2]["controller"]["browserFamily"], "chrome");
    assert_eq!(values[2]["controller"]["profileKey"], "profile-1");
    assert_eq!(values[2]["controller"]["connectionId"], "chrome-connection");
    Ok(())
}

#[test]
fn native_host_takeover_rotates_active_controller_lease() -> Result<(), Box<dyn Error>> {
    let mut input = Vec::new();
    for payload in [
        extension_hello(),
        br#"{
            "type": "extension.controller.acquire",
            "protocolVersion": 1,
            "connectionId": "connection-1",
            "browserFamily": "chrome",
            "profileKey": "profile-1",
            "documentId": "document-1",
            "requestedAtEpochMs": 1000,
            "reason": "initial-connect"
        }"#
        .as_slice(),
        br#"{
            "type": "extension.controller.acquire",
            "protocolVersion": 1,
            "connectionId": "connection-2",
            "browserFamily": "chrome",
            "profileKey": "profile-2",
            "documentId": "document-2",
            "requestedAtEpochMs": 2000,
            "reason": "user-requested-takeover"
        }"#
        .as_slice(),
        br#"{
            "type": "extension.screenshot.capture",
            "protocolVersion": 1,
            "controllerLeaseId": "lease:connection-1",
            "commandId": "command-1",
            "mode": "screen",
            "settingsRevision": "settings-revision-1",
            "openEditor": true,
            "requestedAtEpochMs": 3000
        }"#
        .as_slice(),
    ] {
        input.extend(framed(payload)?);
    }
    let mut stdin = Cursor::new(input);
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    assert!(
        run_with_streams(
            args(&[
                "sniptale-native-agent",
                "--native-host",
                ALLOWED_NATIVE_ORIGIN
            ]),
            &mut stdin,
            &mut stdout,
            &mut stderr,
        )
        .is_ok()
    );

    let values = read_framed_json_values(&stdout)?;
    assert!(stderr.is_empty());
    assert_eq!(values[2]["status"], "granted");
    assert_eq!(values[2]["controllerLeaseId"], "lease:connection-2");
    assert_eq!(values[3]["type"], "app.operation.failed");
    assert_eq!(values[3]["controllerLeaseId"], "lease:connection-1");
    assert_eq!(values[3]["error"]["code"], "stale-controller-lease");
    Ok(())
}

#[test]
fn native_host_rejects_stale_settings_sync_with_typed_failure() -> Result<(), Box<dyn Error>> {
    let mut input = Vec::new();
    for payload in [extension_hello(), extension_settings_sync()] {
        input.extend(framed(payload)?);
    }
    let mut stdin = Cursor::new(input);
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    assert!(
        run_with_streams(
            args(&[
                "sniptale-native-agent",
                "--native-host",
                ALLOWED_NATIVE_ORIGIN
            ]),
            &mut stdin,
            &mut stdout,
            &mut stderr,
        )
        .is_ok()
    );

    let values = read_framed_json_values(&stdout)?;
    assert!(stderr.is_empty());
    assert_eq!(values.len(), 2);
    assert_eq!(values[1]["type"], "app.operation.failed");
    assert_eq!(values[1]["controllerLeaseId"], "lease:connection-1");
    assert_eq!(values[1]["operation"], "settings-sync");
    assert_eq!(values[1]["operationId"], "settings-revision-1");
    assert_eq!(values[1]["phase"], "lease-validation");
    assert_eq!(values[1]["error"]["code"], "stale-controller-lease");
    assert_eq!(values[1]["error"]["recoverable"], true);
    Ok(())
}

#[test]
fn native_host_rejects_expired_settings_sync_with_typed_failure() -> Result<(), Box<dyn Error>> {
    let mut input = Vec::new();
    for payload in [
        extension_hello(),
        br#"{
            "type": "extension.controller.acquire",
            "protocolVersion": 1,
            "connectionId": "connection-1",
            "browserFamily": "chrome",
            "profileKey": "profile-1",
            "documentId": "document-1",
            "requestedAtEpochMs": 1000,
            "reason": "initial-connect"
        }"#
        .as_slice(),
        br#"{
            "type": "extension.settings.sync",
            "protocolVersion": 1,
            "controllerLeaseId": "lease:connection-1",
            "revision": "settings-revision-expired",
            "schemaVersion": 1,
            "generatedAtEpochMs": 302000,
            "settings": {
                "screenshots": {
                    "includeCursor": true
                },
                "video": {
                    "enabled": true,
                    "advanced": {
                        "audioBitrateKbps": 160,
                        "audioSourceMode": "mixed",
                        "frameRate": 30,
                        "includeCursorInVideo": true,
                        "maxDurationMinutes": 240,
                        "preferHardwareEncoder": true,
                        "videoBitrateMbpsOverride": null
                    },
                    "codec": {
                        "audioCodec": "aac",
                        "container": "mp4",
                        "hardwareAcceleration": "prefer",
                        "videoCodec": "h264"
                    },
                    "telemetry": {
                        "collectCursor": true,
                        "collectClicks": true,
                        "collectKeyEvents": true,
                        "collectTypingSpans": true,
                        "collectStaticSignals": true
                    }
                },
                "trayActions": {
                    "revision": "tray-revision-1",
                    "actions": []
                }
            }
        }"#
        .as_slice(),
    ] {
        input.extend(framed(payload)?);
    }
    let mut stdin = Cursor::new(input);
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    assert!(
        run_with_streams(
            args(&[
                "sniptale-native-agent",
                "--native-host",
                ALLOWED_NATIVE_ORIGIN
            ]),
            &mut stdin,
            &mut stdout,
            &mut stderr,
        )
        .is_ok()
    );

    let values = read_framed_json_values(&stdout)?;
    assert!(stderr.is_empty());
    assert_eq!(values[2]["type"], "app.operation.failed");
    assert_eq!(values[2]["operation"], "settings-sync");
    assert_eq!(values[2]["operationId"], "settings-revision-expired");
    assert_eq!(values[2]["phase"], "lease-validation");
    assert_eq!(values[2]["error"]["code"], "stale-controller-lease");
    Ok(())
}

#[test]
fn native_host_rejects_settings_sync_schema_mismatch() -> Result<(), Box<dyn Error>> {
    let mut input = Vec::new();
    for payload in [
        extension_hello(),
        br#"{
            "type": "extension.controller.acquire",
            "protocolVersion": 1,
            "connectionId": "connection-1",
            "browserFamily": "chrome",
            "profileKey": "profile-1",
            "documentId": "document-1",
            "requestedAtEpochMs": 1000,
            "reason": "initial-connect"
        }"#
        .as_slice(),
        br#"{
            "type": "extension.settings.sync",
            "protocolVersion": 1,
            "controllerLeaseId": "lease:connection-1",
            "revision": "settings-revision-bad-schema",
            "schemaVersion": 2,
            "generatedAtEpochMs": 2000,
            "settings": {
                "screenshots": {
                    "includeCursor": true
                },
                "video": {
                    "enabled": true,
                    "advanced": {
                        "audioBitrateKbps": 160,
                        "audioSourceMode": "mixed",
                        "frameRate": 30,
                        "includeCursorInVideo": true,
                        "maxDurationMinutes": 240,
                        "preferHardwareEncoder": true,
                        "videoBitrateMbpsOverride": null
                    },
                    "codec": {
                        "audioCodec": "aac",
                        "container": "mp4",
                        "hardwareAcceleration": "prefer",
                        "videoCodec": "h264"
                    },
                    "telemetry": {
                        "collectCursor": true,
                        "collectClicks": true,
                        "collectKeyEvents": true,
                        "collectTypingSpans": true,
                        "collectStaticSignals": true
                    }
                },
                "trayActions": {
                    "revision": "tray-revision-1",
                    "actions": []
                }
            }
        }"#
        .as_slice(),
    ] {
        input.extend(framed(payload)?);
    }
    let mut stdin = Cursor::new(input);
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    assert!(
        run_with_streams(
            args(&[
                "sniptale-native-agent",
                "--native-host",
                ALLOWED_NATIVE_ORIGIN
            ]),
            &mut stdin,
            &mut stdout,
            &mut stderr,
        )
        .is_ok()
    );

    let values = read_framed_json_values(&stdout)?;
    assert!(stderr.is_empty());
    assert_eq!(values[2]["type"], "app.operation.failed");
    assert_eq!(values[2]["operation"], "settings-sync");
    assert_eq!(values[2]["operationId"], "settings-revision-bad-schema");
    assert_eq!(values[2]["phase"], "schema-version");
    assert_eq!(values[2]["error"]["code"], "incompatible-settings");
    Ok(())
}

#[test]
fn native_host_rejects_settings_sync_with_invalid_hotkey_label() -> Result<(), Box<dyn Error>> {
    let mut input = Vec::new();
    for payload in [
        extension_hello(),
        extension_controller_acquire(),
        br#"{
            "type": "extension.settings.sync",
            "protocolVersion": 1,
            "controllerLeaseId": "lease:connection-1",
            "revision": "settings-revision-bad-hotkey",
            "schemaVersion": 1,
            "generatedAtEpochMs": 2000,
            "settings": {
                "screenshots": {
                    "includeCursor": true
                },
                "video": {
                    "enabled": true,
                    "advanced": {
                        "audioBitrateKbps": 160,
                        "audioSourceMode": "mixed",
                        "frameRate": 30,
                        "includeCursorInVideo": true,
                        "maxDurationMinutes": 240,
                        "preferHardwareEncoder": true,
                        "videoBitrateMbpsOverride": null
                    },
                    "codec": {
                        "audioCodec": "aac",
                        "container": "mp4",
                        "hardwareAcceleration": "prefer",
                        "videoCodec": "h264"
                    },
                    "telemetry": {
                        "collectCursor": true,
                        "collectClicks": true,
                        "collectKeyEvents": true,
                        "collectTypingSpans": true,
                        "collectStaticSignals": true
                    }
                },
                "trayActions": {
                    "actions": [{
                        "enabled": true,
                        "id": "screenshot-screen",
                        "kind": "capture-screenshot",
                        "label": "Capture",
                        "shortcutLabel": "Ctrl++R",
                        "offlineCapable": false
                    }],
                    "revision": "tray-revision-bad-hotkey"
                }
            }
        }"#
        .as_slice(),
    ] {
        input.extend(framed(payload)?);
    }
    let mut stdin = Cursor::new(input);
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    assert!(
        run_with_streams(
            args(&[
                "sniptale-native-agent",
                "--native-host",
                ALLOWED_NATIVE_ORIGIN
            ]),
            &mut stdin,
            &mut stdout,
            &mut stderr,
        )
        .is_ok()
    );

    let values = read_framed_json_values(&stdout)?;
    assert!(stderr.is_empty());
    assert_eq!(values[2]["type"], "app.operation.failed");
    assert_eq!(values[2]["operation"], "settings-sync");
    assert_eq!(values[2]["operationId"], "settings-revision-bad-hotkey");
    assert_eq!(values[2]["phase"], "hotkeys");
    assert_eq!(values[2]["error"]["code"], "incompatible-settings");
    Ok(())
}
