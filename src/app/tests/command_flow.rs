use super::*;

#[test]
fn native_host_fails_operational_commands_with_typed_error() -> Result<(), Box<dyn Error>> {
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
            "type": "extension.screenshot.capture",
            "protocolVersion": 1,
            "controllerLeaseId": "lease:connection-1",
            "commandId": "command-1",
            "mode": "screen",
            "settingsRevision": "settings-revision-1",
            "openEditor": true,
            "requestedAtEpochMs": 4000
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
    assert_eq!(values[3]["type"], "app.operation.failed");
    assert_eq!(values[3]["operation"], "screenshot");
    assert_eq!(values[3]["operationId"], "command-1");
    assert_eq!(values[3]["error"]["code"], "unsupported-capability");
    Ok(())
}

#[test]
fn native_host_accepts_dispatched_screenshot_command_once() -> Result<(), Box<dyn Error>> {
    let mut session = settings_synced_native_host_session()?;
    let message = parse_wire_message(extension_screenshot_capture_command())?;
    let value: Value = serde_json::from_slice(extension_screenshot_capture_command())?;
    let mut effects = CommandDispatchEffects {
        screenshot_outcome: Ok(NativeHostCommandOutcome::Accepted),
        ..CommandDispatchEffects::default()
    };

    let first = responses_for_message_with_effects(&mut session, &message, &value, &mut effects);
    let second = responses_for_message_with_effects(&mut session, &message, &value, &mut effects);

    assert_eq!(effects.screenshot_starts, 1);
    assert_eq!(first, second);
    assert_eq!(first.len(), 1);
    assert_eq!(first[0]["type"], "app.command.accepted");
    assert_eq!(first[0]["controllerLeaseId"], "lease:connection-1");
    assert_eq!(first[0]["commandId"], "command-1");
    assert_eq!(first[0]["operation"], "screenshot");
    assert_eq!(first[0]["acceptedAtEpochMs"], 4_000);

    let parsed = parse_wire_message(&serde_json::to_vec(&first[0])?)?;
    let WireMessage::AppCommandAccepted(accepted) = parsed else {
        return Err("expected app.command.accepted".into());
    };
    assert_eq!(accepted.command_id, "command-1");
    Ok(())
}

#[test]
fn native_host_accepts_screenshot_command_with_start_message() -> Result<(), Box<dyn Error>> {
    let mut session = settings_synced_native_host_session()?;
    let message = parse_wire_message(extension_screenshot_capture_command())?;
    let value: Value = serde_json::from_slice(extension_screenshot_capture_command())?;
    let mut effects = CommandDispatchEffects {
        screenshot_outcome: Ok(NativeHostCommandOutcome::AcceptedWithEvents(vec![
            NativeHostCommandEvent::ScreenshotStart(AppScreenshotStartMessage {
                protocol_version: 1,
                controller_lease_id: "lease:connection-1".to_owned(),
                capture_id: "command-1".to_owned(),
                mode: ScreenshotMode::Screen,
                filename: "capture.png".to_owned(),
                mime_type: ScreenshotMimeType::ImagePng,
                total_bytes: 11,
                width: 10,
                height: 10,
                chunk_count: 1,
                sha256: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                    .to_owned(),
                open_editor: true,
                captured_at_epoch_ms: 4_000,
            }),
        ])),
        ..CommandDispatchEffects::default()
    };

    let first = responses_for_message_with_effects(&mut session, &message, &value, &mut effects);
    let second = responses_for_message_with_effects(&mut session, &message, &value, &mut effects);

    assert_eq!(first.len(), 2);
    assert_eq!(first[0]["type"], "app.command.accepted");
    assert_eq!(first[1]["type"], "app.screenshot.start");
    assert_eq!(first[1]["captureId"], "command-1");
    assert_eq!(second.len(), 1);
    assert_eq!(second[0], first[0]);
    assert!(second[0].get("captureId").is_none());
    assert!(second[0].get("totalBytes").is_none());
    assert!(second[0].get("base64").is_none());
    assert!(second[0].get("sha256").is_none());
    assert_eq!(effects.screenshot_starts, 1);
    Ok(())
}

#[test]
fn outbound_command_events_require_matching_controller_lease() -> Result<(), Box<dyn Error>> {
    let shared = NativeHostSharedState::default();
    let mut owner = NativeHostSession::with_shared_state(shared.clone());
    let mut observer = NativeHostSession::with_shared_state(shared);
    let hello_message = parse_wire_message(extension_hello())?;
    let hello_value: Value = serde_json::from_slice(extension_hello())?;
    responses_for_message(&mut owner, &hello_message, &hello_value);
    responses_for_message(&mut observer, &hello_message, &hello_value);
    let acquire_message = parse_wire_message(extension_controller_acquire())?;
    let acquire_value: Value = serde_json::from_slice(extension_controller_acquire())?;
    let mut effects = CommandDispatchEffects::default();
    responses_for_message_with_effects(&mut owner, &acquire_message, &acquire_value, &mut effects);

    let event = NativeHostCommandEvent::ScreenshotStart(AppScreenshotStartMessage {
        protocol_version: 1,
        controller_lease_id: "lease:connection-1".to_owned(),
        capture_id: "capture-1".to_owned(),
        mode: ScreenshotMode::Screen,
        filename: "capture.png".to_owned(),
        mime_type: ScreenshotMimeType::ImagePng,
        total_bytes: 11,
        width: 10,
        height: 10,
        chunk_count: 1,
        sha256: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned(),
        open_editor: true,
        captured_at_epoch_ms: 4_000,
    });

    let owner_responses = responses_for_command_event(&owner, event.clone());
    let observer_responses = responses_for_command_event(&observer, event);

    assert_eq!(owner_responses.len(), 1);
    assert_eq!(owner_responses[0]["type"], "app.screenshot.start");
    assert!(observer_responses.is_empty());
    Ok(())
}

#[test]
fn outbound_command_failures_require_matching_controller_lease() -> Result<(), Box<dyn Error>> {
    let shared = NativeHostSharedState::default();
    let mut owner = NativeHostSession::with_shared_state(shared.clone());
    let mut observer = NativeHostSession::with_shared_state(shared);
    let hello_message = parse_wire_message(extension_hello())?;
    let hello_value: Value = serde_json::from_slice(extension_hello())?;
    responses_for_message(&mut owner, &hello_message, &hello_value);
    responses_for_message(&mut observer, &hello_message, &hello_value);
    let acquire_message = parse_wire_message(extension_controller_acquire())?;
    let acquire_value: Value = serde_json::from_slice(extension_controller_acquire())?;
    let mut effects = CommandDispatchEffects::default();
    responses_for_message_with_effects(&mut owner, &acquire_message, &acquire_value, &mut effects);

    let failure = NativeHostCommandFailureEvent {
        controller_lease_id: Some("lease:connection-1".to_owned()),
        operation: OperationFailedOperation::Telemetry,
        operation_id: Some("recording-1".to_owned()),
        phase: "start",
        kind: OperationalErrorKind::QueueBackpressure,
        occurred_at_epoch_ms: 4_000,
    };
    let unroutable_failure = NativeHostCommandFailureEvent {
        controller_lease_id: None,
        ..failure.clone()
    };

    let owner_responses = responses_for_command_failure(&owner, &failure);
    let observer_responses = responses_for_command_failure(&observer, &failure);
    let unroutable_responses = responses_for_command_failure(&owner, &unroutable_failure);

    assert_eq!(owner_responses.len(), 1);
    assert_eq!(owner_responses[0]["type"], "app.operation.failed");
    assert_eq!(
        owner_responses[0]["controllerLeaseId"],
        "lease:connection-1"
    );
    assert_eq!(owner_responses[0]["operation"], "telemetry");
    assert!(observer_responses.is_empty());
    assert!(unroutable_responses.is_empty());
    Ok(())
}

#[test]
fn native_host_accepts_recording_start_with_followup_messages() -> Result<(), Box<dyn Error>> {
    let mut session = settings_synced_native_host_session()?;
    let message = parse_wire_message(extension_recording_start_command())?;
    let value: Value = serde_json::from_slice(extension_recording_start_command())?;
    let mut effects = CommandDispatchEffects {
        recording_outcome: Ok(NativeHostCommandOutcome::AcceptedWithEvents(vec![
            NativeHostCommandEvent::RecordingProgress(AppRecordingProgressMessage {
                protocol_version: 1,
                controller_lease_id: "lease:connection-1".to_owned(),
                recording_id: "recording-command-1".to_owned(),
                duration_ms: 0,
                bytes_written: 0,
                status: RecordingProgressStatus::Recording,
            }),
        ])),
        ..CommandDispatchEffects::default()
    };

    let first = responses_for_message_with_effects(&mut session, &message, &value, &mut effects);
    let second = responses_for_message_with_effects(&mut session, &message, &value, &mut effects);

    assert_eq!(first.len(), 2);
    assert_eq!(first[0]["type"], "app.command.accepted");
    assert_eq!(first[0]["operation"], "recording");
    assert_eq!(first[1]["type"], "app.recording.progress");
    assert_eq!(first[1]["recordingId"], "recording-command-1");
    assert_eq!(second.len(), 1);
    assert_eq!(second[0], first[0]);
    assert_eq!(effects.recording_starts, 1);
    assert_eq!(
        effects.last_recording_policy,
        Some(RecordingStartPolicy {
            max_duration_ms: 14_400_000,
            telemetry: crate::telemetry::RecordingTelemetryPolicy::all_enabled_for_tests()
        })
    );
    Ok(())
}

#[test]
fn recording_start_policy_uses_accepted_telemetry_settings() -> Result<(), Box<dyn Error>> {
    let mut session = connected_native_host_session()?;
    let mut settings_value: Value = serde_json::from_slice(extension_settings_sync())?;
    settings_value["settings"]["video"]["telemetry"]["collectCursor"] = Value::Bool(false);
    settings_value["settings"]["video"]["telemetry"]["collectClicks"] = Value::Bool(true);
    settings_value["settings"]["video"]["telemetry"]["collectKeyEvents"] = Value::Bool(false);
    settings_value["settings"]["video"]["telemetry"]["collectTypingSpans"] = Value::Bool(true);
    settings_value["settings"]["video"]["telemetry"]["collectStaticSignals"] = Value::Bool(false);
    let settings_bytes = serde_json::to_vec(&settings_value)?;
    let settings_message = parse_wire_message(&settings_bytes)?;
    responses_for_message(&mut session, &settings_message, &settings_value);

    let message = parse_wire_message(extension_recording_start_command())?;
    let value: Value = serde_json::from_slice(extension_recording_start_command())?;
    let mut effects = CommandDispatchEffects {
        recording_outcome: Ok(NativeHostCommandOutcome::Accepted),
        ..CommandDispatchEffects::default()
    };

    let responses =
        responses_for_message_with_effects(&mut session, &message, &value, &mut effects);

    assert_eq!(responses[0]["type"], "app.command.accepted");
    assert_eq!(
        effects.last_recording_policy,
        Some(RecordingStartPolicy {
            max_duration_ms: 14_400_000,
            telemetry: crate::telemetry::RecordingTelemetryPolicy {
                collect_cursor: false,
                collect_clicks: true,
                collect_key_events: false,
                collect_typing_spans: true,
                collect_static_signals: false,
                telemetry_non_fatal: true
            }
        })
    );
    Ok(())
}

#[test]
fn native_host_reports_command_dispatch_failure() -> Result<(), Box<dyn Error>> {
    let mut session = settings_synced_native_host_session()?;
    let message = parse_wire_message(extension_screenshot_capture_command())?;
    let value: Value = serde_json::from_slice(extension_screenshot_capture_command())?;
    let mut effects = CommandDispatchEffects {
        screenshot_outcome: Err(NativeHostSideEffectError::CommandDispatch),
        ..CommandDispatchEffects::default()
    };

    let responses =
        responses_for_message_with_effects(&mut session, &message, &value, &mut effects);

    assert_eq!(effects.screenshot_starts, 1);
    assert_eq!(responses.len(), 1);
    assert_eq!(responses[0]["type"], "app.operation.failed");
    assert_eq!(responses[0]["operation"], "screenshot");
    assert_eq!(responses[0]["operationId"], "command-1");
    assert_eq!(responses[0]["phase"], "screenshot");
    assert_eq!(responses[0]["error"]["code"], "command-dispatch-failed");
    assert_eq!(responses[0]["error"]["recoverable"], true);
    Ok(())
}

#[test]
fn native_host_reports_worker_queue_backpressure() -> Result<(), Box<dyn Error>> {
    let mut session = settings_synced_native_host_session()?;
    let message = parse_wire_message(extension_screenshot_capture_command())?;
    let value: Value = serde_json::from_slice(extension_screenshot_capture_command())?;
    let mut effects = CommandDispatchEffects {
        screenshot_outcome: Err(NativeHostSideEffectError::WorkerQueueFull),
        ..CommandDispatchEffects::default()
    };

    let responses =
        responses_for_message_with_effects(&mut session, &message, &value, &mut effects);

    assert_eq!(effects.screenshot_starts, 1);
    assert_eq!(responses.len(), 1);
    assert_eq!(responses[0]["type"], "app.operation.failed");
    assert_eq!(responses[0]["operation"], "screenshot");
    assert_eq!(responses[0]["operationId"], "command-1");
    assert_eq!(responses[0]["phase"], "screenshot");
    assert_eq!(responses[0]["error"]["code"], "queue-backpressure");
    assert_eq!(responses[0]["error"]["recoverable"], true);
    Ok(())
}

#[test]
fn native_host_reports_stopped_worker_dispatch_failure() -> Result<(), Box<dyn Error>> {
    let mut session = settings_synced_native_host_session()?;
    let message = parse_wire_message(extension_recording_start_command())?;
    let value: Value = serde_json::from_slice(extension_recording_start_command())?;
    let mut effects = CommandDispatchEffects {
        recording_outcome: Err(NativeHostSideEffectError::WorkerStopped),
        ..CommandDispatchEffects::default()
    };

    let responses =
        responses_for_message_with_effects(&mut session, &message, &value, &mut effects);

    assert_eq!(effects.recording_starts, 1);
    assert_eq!(responses.len(), 1);
    assert_eq!(responses[0]["type"], "app.operation.failed");
    assert_eq!(responses[0]["operation"], "recording");
    assert_eq!(responses[0]["operationId"], "command-1");
    assert_eq!(responses[0]["phase"], "recording");
    assert_eq!(responses[0]["error"]["code"], "command-dispatch-failed");
    assert_eq!(responses[0]["error"]["recoverable"], true);
    Ok(())
}

#[test]
fn native_host_reports_region_selection_cancelled() -> Result<(), Box<dyn Error>> {
    let mut session = settings_synced_native_host_session()?;
    let message = parse_wire_message(extension_screenshot_capture_command())?;
    let value: Value = serde_json::from_slice(extension_screenshot_capture_command())?;
    let mut effects = CommandDispatchEffects {
        screenshot_outcome: Err(NativeHostSideEffectError::ScreenshotRegionCancelled),
        ..CommandDispatchEffects::default()
    };

    let responses =
        responses_for_message_with_effects(&mut session, &message, &value, &mut effects);

    assert_eq!(effects.screenshot_starts, 1);
    assert_eq!(responses.len(), 1);
    assert_eq!(responses[0]["type"], "app.operation.failed");
    assert_eq!(responses[0]["operation"], "screenshot");
    assert_eq!(responses[0]["phase"], "screenshot");
    assert_eq!(responses[0]["error"]["code"], "user-cancelled");
    assert_eq!(responses[0]["error"]["recoverable"], true);
    Ok(())
}

#[test]
fn native_host_reports_invalid_region_selection() -> Result<(), Box<dyn Error>> {
    let mut session = settings_synced_native_host_session()?;
    let message = parse_wire_message(extension_screenshot_capture_command())?;
    let value: Value = serde_json::from_slice(extension_screenshot_capture_command())?;
    let mut effects = CommandDispatchEffects {
        screenshot_outcome: Err(NativeHostSideEffectError::ScreenshotInvalidSelection),
        ..CommandDispatchEffects::default()
    };

    let responses =
        responses_for_message_with_effects(&mut session, &message, &value, &mut effects);

    assert_eq!(effects.screenshot_starts, 1);
    assert_eq!(responses.len(), 1);
    assert_eq!(responses[0]["type"], "app.operation.failed");
    assert_eq!(responses[0]["operation"], "screenshot");
    assert_eq!(responses[0]["phase"], "screenshot");
    assert_eq!(responses[0]["error"]["code"], "invalid-selection");
    assert_eq!(responses[0]["error"]["recoverable"], true);
    Ok(())
}

#[test]
fn native_host_reports_recording_disk_pressure_failure() -> Result<(), Box<dyn Error>> {
    let mut session = settings_synced_native_host_session()?;
    let message = parse_wire_message(extension_recording_start_command())?;
    let value: Value = serde_json::from_slice(extension_recording_start_command())?;
    let mut effects = CommandDispatchEffects {
        recording_outcome: Err(NativeHostSideEffectError::RecordingDiskPressure),
        ..CommandDispatchEffects::default()
    };

    let responses =
        responses_for_message_with_effects(&mut session, &message, &value, &mut effects);

    assert_eq!(effects.recording_starts, 1);
    assert_eq!(responses.len(), 1);
    assert_eq!(responses[0]["type"], "app.operation.failed");
    assert_eq!(responses[0]["operation"], "recording");
    assert_eq!(responses[0]["operationId"], "command-1");
    assert_eq!(responses[0]["phase"], "recording");
    assert_eq!(responses[0]["error"]["code"], "disk-pressure");
    assert_eq!(responses[0]["error"]["recoverable"], true);
    Ok(())
}

#[test]
fn native_host_rejects_command_settings_revision_mismatch() -> Result<(), Box<dyn Error>> {
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
            "type": "extension.screenshot.capture",
            "protocolVersion": 1,
            "controllerLeaseId": "lease:connection-1",
            "commandId": "command-1",
            "mode": "screen",
            "settingsRevision": "stale-settings-revision",
            "openEditor": true,
            "requestedAtEpochMs": 4000
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
    assert_eq!(values[3]["type"], "app.operation.failed");
    assert_eq!(values[3]["operation"], "screenshot");
    assert_eq!(values[3]["operationId"], "command-1");
    assert_eq!(values[3]["phase"], "settings-revision");
    assert_eq!(values[3]["error"]["code"], "incompatible-settings");
    Ok(())
}

#[test]
fn native_host_rejects_command_after_controller_lease_expiry() -> Result<(), Box<dyn Error>> {
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
            "type": "extension.screenshot.capture",
            "protocolVersion": 1,
            "controllerLeaseId": "lease:connection-1",
            "commandId": "command-expired-lease",
            "mode": "screen",
            "settingsRevision": "settings-revision-1",
            "openEditor": true,
            "requestedAtEpochMs": 302000
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
    assert_eq!(values[3]["type"], "app.operation.failed");
    assert_eq!(values[3]["operation"], "screenshot");
    assert_eq!(values[3]["operationId"], "command-expired-lease");
    assert_eq!(values[3]["phase"], "lease-validation");
    assert_eq!(values[3]["error"]["code"], "stale-controller-lease");
    Ok(())
}

#[test]
fn native_host_reports_unavailable_lease_state_for_command_validation() -> Result<(), Box<dyn Error>>
{
    let shared_state = NativeHostSharedState::default();
    let mut session = settings_synced_native_host_session_with_shared_state(shared_state.clone())?;
    shared_state.poison_lease_book_for_tests();
    let message = parse_wire_message(extension_screenshot_capture_command())?;
    let value: Value = serde_json::from_slice(extension_screenshot_capture_command())?;
    let mut effects = CommandDispatchEffects {
        screenshot_outcome: Ok(NativeHostCommandOutcome::Accepted),
        ..CommandDispatchEffects::default()
    };

    let responses =
        responses_for_message_with_effects(&mut session, &message, &value, &mut effects);

    assert_eq!(effects.screenshot_starts, 0);
    assert_eq!(responses.len(), 1);
    assert_eq!(responses[0]["type"], "app.operation.failed");
    assert_eq!(responses[0]["operation"], "screenshot");
    assert_eq!(responses[0]["phase"], "lease-validation");
    assert_eq!(responses[0]["error"]["code"], "storage-failed");
    assert_ne!(responses[0]["error"]["code"], "stale-controller-lease");
    Ok(())
}

#[test]
fn native_host_replays_previous_command_result_for_duplicate_id() -> Result<(), Box<dyn Error>> {
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
            "type": "extension.screenshot.capture",
            "protocolVersion": 1,
            "controllerLeaseId": "lease:connection-1",
            "commandId": "command-1",
            "mode": "screen",
            "settingsRevision": "settings-revision-1",
            "openEditor": true,
            "requestedAtEpochMs": 4000
        }"#
        .as_slice(),
        br#"{
            "type": "extension.screenshot.capture",
            "protocolVersion": 1,
            "controllerLeaseId": "lease:connection-1",
            "commandId": "command-1",
            "mode": "region",
            "settingsRevision": "stale-settings-revision",
            "openEditor": false,
            "requestedAtEpochMs": 9000
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
    assert_eq!(values[3]["error"]["code"], "unsupported-capability");
    assert_eq!(values[3], values[4]);
    Ok(())
}

#[test]
fn native_host_does_not_replay_cached_command_after_lease_expiry() -> Result<(), Box<dyn Error>> {
    let mut input = Vec::new();
    for payload in [
        extension_hello(),
        extension_controller_acquire(),
        extension_settings_sync(),
        br#"{
            "type": "extension.screenshot.capture",
            "protocolVersion": 1,
            "controllerLeaseId": "lease:connection-1",
            "commandId": "command-1",
            "mode": "screen",
            "settingsRevision": "settings-revision-1",
            "openEditor": true,
            "requestedAtEpochMs": 4000
        }"#
        .as_slice(),
        br#"{
            "type": "extension.ping",
            "protocolVersion": 1,
            "nonce": "nonce-expired",
            "sentAtEpochMs": 302000
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
            "requestedAtEpochMs": 302000
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
    assert_eq!(values[3]["error"]["code"], "unsupported-capability");
    assert_eq!(values[5]["phase"], "lease-validation");
    assert_eq!(values[5]["error"]["code"], "stale-controller-lease");
    assert_ne!(values[3], values[5]);
    Ok(())
}

#[test]
fn native_host_rejects_unknown_screenshot_ack_with_typed_failure() -> Result<(), Box<dyn Error>> {
    let mut input = Vec::new();
    for payload in [
        extension_hello(),
        extension_controller_acquire(),
        extension_settings_sync(),
        br#"{
            "type": "extension.screenshot.ack",
            "protocolVersion": 1,
            "controllerLeaseId": "lease:connection-1",
            "captureId": "capture-not-pending",
            "assetId": "asset-1"
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
    assert_eq!(values[3]["type"], "app.operation.failed");
    assert_eq!(values[3]["operation"], "screenshot");
    assert_eq!(values[3]["operationId"], "capture-not-pending");
    assert_eq!(values[3]["phase"], "transfer-state");
    assert_eq!(values[3]["error"]["code"], "duplicate-or-replay");
    assert_eq!(values[3]["occurredAtEpochMs"], 2000);
    Ok(())
}

#[test]
fn native_host_rejects_recording_chunk_request_after_lease_expiry() -> Result<(), Box<dyn Error>> {
    let mut input = Vec::new();
    for payload in [
        extension_hello(),
        extension_controller_acquire(),
        extension_settings_sync(),
        br#"{
            "type": "extension.ping",
            "protocolVersion": 1,
            "nonce": "nonce-expired",
            "sentAtEpochMs": 302000
        }"#
        .as_slice(),
        br#"{
            "type": "extension.recording.chunkRequest",
            "protocolVersion": 1,
            "controllerLeaseId": "lease:connection-1",
            "recordingId": "recording-not-pending",
            "chunkIndex": 0
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
    assert_eq!(values[4]["type"], "app.operation.failed");
    assert_eq!(values[4]["operation"], "recording");
    assert_eq!(values[4]["operationId"], "recording-not-pending");
    assert_eq!(values[4]["phase"], "lease-validation");
    assert_eq!(values[4]["error"]["code"], "stale-controller-lease");
    assert_eq!(values[4]["occurredAtEpochMs"], 302_000);
    Ok(())
}

#[test]
fn native_host_reports_unavailable_lease_state_for_transfer_validation()
-> Result<(), Box<dyn Error>> {
    let shared_state = NativeHostSharedState::default();
    let mut session = settings_synced_native_host_session_with_shared_state(shared_state.clone())?;
    shared_state.poison_lease_book_for_tests();
    let request = br#"{
        "type": "extension.screenshot.chunkRequest",
        "protocolVersion": 1,
        "controllerLeaseId": "lease:connection-1",
        "captureId": "capture-1",
        "chunkIndex": 0
    }"#;
    let message = parse_wire_message(request)?;
    let value: Value = serde_json::from_slice(request)?;
    let mut effects = TransferChunkEffects {
        screenshot_results: VecDeque::from([Ok(Some(native_test_transfer_chunk(0, false)))]),
        ..TransferChunkEffects::default()
    };

    let responses =
        responses_for_message_with_effects(&mut session, &message, &value, &mut effects);

    assert!(effects.screenshot_requests.is_empty());
    assert_eq!(responses.len(), 1);
    assert_eq!(responses[0]["type"], "app.operation.failed");
    assert_eq!(responses[0]["operation"], "screenshot");
    assert_eq!(responses[0]["operationId"], "capture-1");
    assert_eq!(responses[0]["phase"], "lease-validation");
    assert_eq!(responses[0]["error"]["code"], "storage-failed");
    assert_ne!(responses[0]["error"]["code"], "stale-controller-lease");
    Ok(())
}

#[test]
fn native_host_returns_screenshot_chunk_for_pending_transfer() -> Result<(), Box<dyn Error>> {
    let mut session = connected_native_host_session()?;
    let request = br#"{
        "type": "extension.screenshot.chunkRequest",
        "protocolVersion": 1,
        "controllerLeaseId": "lease:connection-1",
        "captureId": "capture-1",
        "chunkIndex": 0
    }"#;
    let message = parse_wire_message(request)?;
    let value: Value = serde_json::from_slice(request)?;
    let mut effects = TransferChunkEffects {
        screenshot_results: VecDeque::from([Ok(Some(native_test_transfer_chunk(0, false)))]),
        ..TransferChunkEffects::default()
    };

    let responses =
        responses_for_message_with_effects(&mut session, &message, &value, &mut effects);

    assert_eq!(responses.len(), 1);
    assert_eq!(responses[0]["type"], "app.screenshot.chunk");
    assert_eq!(responses[0]["controllerLeaseId"], "lease:connection-1");
    assert_eq!(responses[0]["captureId"], "capture-1");
    assert_eq!(responses[0]["chunkIndex"], 0);
    assert_eq!(responses[0]["chunkRawBytes"], 11);
    assert_eq!(responses[0]["base64"], "aGVsbG8gd29ybGQ=");
    assert_eq!(effects.screenshot_requests.len(), 1);
    assert_eq!(effects.screenshot_requests[0].0, "capture-1");
    assert_eq!(effects.screenshot_requests[0].1, 0);
    Ok(())
}

#[test]
fn native_host_commits_screenshot_after_final_chunk() -> Result<(), Box<dyn Error>> {
    let mut session = connected_native_host_session()?;
    let request = br#"{
        "type": "extension.screenshot.chunkRequest",
        "protocolVersion": 1,
        "controllerLeaseId": "lease:connection-1",
        "captureId": "capture-1",
        "chunkIndex": 0
    }"#;
    let message = parse_wire_message(request)?;
    let value: Value = serde_json::from_slice(request)?;
    let mut effects = TransferChunkEffects {
        screenshot_results: VecDeque::from([Ok(Some(native_test_transfer_chunk(0, true)))]),
        ..TransferChunkEffects::default()
    };

    let responses =
        responses_for_message_with_effects(&mut session, &message, &value, &mut effects);

    assert_eq!(responses.len(), 2);
    assert_eq!(responses[0]["type"], "app.screenshot.chunk");
    assert_eq!(responses[0]["captureId"], "capture-1");
    assert_eq!(responses[1]["type"], "app.screenshot.commit");
    assert_eq!(responses[1]["controllerLeaseId"], "lease:connection-1");
    assert_eq!(responses[1]["captureId"], "capture-1");
    Ok(())
}

#[test]
fn native_host_rejects_replayed_screenshot_chunk_request_with_typed_failure()
-> Result<(), Box<dyn Error>> {
    let mut session = connected_native_host_session()?;
    let request = br#"{
        "type": "extension.screenshot.chunkRequest",
        "protocolVersion": 1,
        "controllerLeaseId": "lease:connection-1",
        "captureId": "capture-1",
        "chunkIndex": 0
    }"#;
    let message = parse_wire_message(request)?;
    let value: Value = serde_json::from_slice(request)?;
    let mut effects = TransferChunkEffects {
        screenshot_results: VecDeque::from([
            Ok(Some(native_test_transfer_chunk(0, false))),
            Err(NativeHostTransferChunkError::Replay),
        ]),
        ..TransferChunkEffects::default()
    };

    let first = responses_for_message_with_effects(&mut session, &message, &value, &mut effects);
    let second = responses_for_message_with_effects(&mut session, &message, &value, &mut effects);

    assert_eq!(first.len(), 1);
    assert_eq!(first[0]["type"], "app.screenshot.chunk");
    assert_eq!(first[0]["base64"], "aGVsbG8gd29ybGQ=");
    assert_eq!(second.len(), 1);
    assert_eq!(second[0]["type"], "app.operation.failed");
    assert_eq!(second[0]["operation"], "screenshot");
    assert_eq!(second[0]["operationId"], "capture-1");
    assert_eq!(second[0]["phase"], "transfer-state");
    assert_eq!(second[0]["error"]["code"], "duplicate-or-replay");
    assert!(second[0].get("base64").is_none());
    Ok(())
}

#[test]
fn native_host_rejects_screenshot_chunk_read_for_missing_transfer() -> Result<(), Box<dyn Error>> {
    let mut session = connected_native_host_session()?;
    let request = br#"{
        "type": "extension.screenshot.chunkRequest",
        "protocolVersion": 1,
        "controllerLeaseId": "lease:connection-1",
        "captureId": "capture-1",
        "chunkIndex": 0
    }"#;
    let message = parse_wire_message(request)?;
    let value: Value = serde_json::from_slice(request)?;
    let mut effects = TransferChunkEffects::default();

    let responses =
        responses_for_message_with_effects(&mut session, &message, &value, &mut effects);

    assert_eq!(responses.len(), 1);
    assert_eq!(responses[0]["type"], "app.operation.failed");
    assert_eq!(responses[0]["operation"], "screenshot");
    assert_eq!(responses[0]["operationId"], "capture-1");
    assert_eq!(responses[0]["phase"], "transfer-state");
    assert_eq!(responses[0]["error"]["code"], "duplicate-or-replay");
    assert_eq!(responses[0]["error"]["recoverable"], true);
    assert!(responses[0].get("base64").is_none());
    Ok(())
}

#[test]
fn native_host_rejects_screenshot_chunk_index_with_typed_failure() -> Result<(), Box<dyn Error>> {
    let mut session = connected_native_host_session()?;
    let request = br#"{
        "type": "extension.screenshot.chunkRequest",
        "protocolVersion": 1,
        "controllerLeaseId": "lease:connection-1",
        "captureId": "capture-1",
        "chunkIndex": 7
    }"#;
    let message = parse_wire_message(request)?;
    let value: Value = serde_json::from_slice(request)?;
    let mut effects = TransferChunkEffects {
        screenshot_results: VecDeque::from([Err(NativeHostTransferChunkError::ChunkOutOfBounds)]),
        ..TransferChunkEffects::default()
    };

    let responses =
        responses_for_message_with_effects(&mut session, &message, &value, &mut effects);

    assert_eq!(responses.len(), 1);
    assert_eq!(responses[0]["type"], "app.operation.failed");
    assert_eq!(responses[0]["operation"], "screenshot");
    assert_eq!(responses[0]["operationId"], "capture-1");
    assert_eq!(responses[0]["phase"], "transfer-index");
    assert_eq!(responses[0]["error"]["code"], "chunk-out-of-bounds");
    assert_eq!(responses[0]["error"]["recoverable"], true);
    assert!(responses[0].get("base64").is_none());
    Ok(())
}

#[test]
fn native_host_returns_recording_chunk_for_pending_transfer() -> Result<(), Box<dyn Error>> {
    let mut session = connected_native_host_session()?;
    let request = br#"{
        "type": "extension.recording.chunkRequest",
        "protocolVersion": 1,
        "controllerLeaseId": "lease:connection-1",
        "recordingId": "recording-1",
        "chunkIndex": 0
    }"#;
    let message = parse_wire_message(request)?;
    let value: Value = serde_json::from_slice(request)?;
    let mut effects = TransferChunkEffects {
        recording_results: VecDeque::from([Ok(Some(encoded_test_chunk(0)))]),
        ..TransferChunkEffects::default()
    };

    let responses =
        responses_for_message_with_effects(&mut session, &message, &value, &mut effects);

    assert_eq!(responses.len(), 1);
    assert_eq!(responses[0]["type"], "app.recording.chunk");
    assert_eq!(responses[0]["controllerLeaseId"], "lease:connection-1");
    assert_eq!(responses[0]["recordingId"], "recording-1");
    assert_eq!(responses[0]["chunkIndex"], 0);
    assert_eq!(responses[0]["chunkRawBytes"], 11);
    assert_eq!(responses[0]["base64"], "aGVsbG8gd29ybGQ=");
    assert_eq!(effects.recording_requests.len(), 1);
    assert_eq!(effects.recording_requests[0].0, "recording-1");
    assert_eq!(effects.recording_requests[0].1, 0);
    Ok(())
}

#[test]
fn native_host_rejects_replayed_recording_chunk_request_with_typed_failure()
-> Result<(), Box<dyn Error>> {
    let mut session = connected_native_host_session()?;
    let request = br#"{
        "type": "extension.recording.chunkRequest",
        "protocolVersion": 1,
        "controllerLeaseId": "lease:connection-1",
        "recordingId": "recording-1",
        "chunkIndex": 0
    }"#;
    let message = parse_wire_message(request)?;
    let value: Value = serde_json::from_slice(request)?;
    let mut effects = TransferChunkEffects {
        recording_results: VecDeque::from([
            Ok(Some(encoded_test_chunk(0))),
            Err(NativeHostTransferChunkError::Replay),
        ]),
        ..TransferChunkEffects::default()
    };

    let first = responses_for_message_with_effects(&mut session, &message, &value, &mut effects);
    let second = responses_for_message_with_effects(&mut session, &message, &value, &mut effects);

    assert_eq!(first.len(), 1);
    assert_eq!(first[0]["type"], "app.recording.chunk");
    assert_eq!(first[0]["base64"], "aGVsbG8gd29ybGQ=");
    assert_eq!(second.len(), 1);
    assert_eq!(second[0]["type"], "app.operation.failed");
    assert_eq!(second[0]["operation"], "recording");
    assert_eq!(second[0]["operationId"], "recording-1");
    assert_eq!(second[0]["phase"], "transfer-state");
    assert_eq!(second[0]["error"]["code"], "duplicate-or-replay");
    assert!(second[0].get("base64").is_none());
    Ok(())
}

#[test]
fn native_host_rejects_recording_chunk_read_for_missing_transfer() -> Result<(), Box<dyn Error>> {
    let mut session = connected_native_host_session()?;
    let request = br#"{
        "type": "extension.recording.chunkRequest",
        "protocolVersion": 1,
        "controllerLeaseId": "lease:connection-1",
        "recordingId": "recording-1",
        "chunkIndex": 0
    }"#;
    let message = parse_wire_message(request)?;
    let value: Value = serde_json::from_slice(request)?;
    let mut effects = TransferChunkEffects::default();

    let responses =
        responses_for_message_with_effects(&mut session, &message, &value, &mut effects);

    assert_eq!(responses.len(), 1);
    assert_eq!(responses[0]["type"], "app.operation.failed");
    assert_eq!(responses[0]["operation"], "recording");
    assert_eq!(responses[0]["operationId"], "recording-1");
    assert_eq!(responses[0]["phase"], "transfer-state");
    assert_eq!(responses[0]["error"]["code"], "duplicate-or-replay");
    assert_eq!(responses[0]["error"]["recoverable"], true);
    assert!(responses[0].get("base64").is_none());
    Ok(())
}

#[test]
fn native_host_rejects_recording_chunk_index_with_typed_failure() -> Result<(), Box<dyn Error>> {
    let mut session = connected_native_host_session()?;
    let request = br#"{
        "type": "extension.recording.chunkRequest",
        "protocolVersion": 1,
        "controllerLeaseId": "lease:connection-1",
        "recordingId": "recording-1",
        "chunkIndex": 7
    }"#;
    let message = parse_wire_message(request)?;
    let value: Value = serde_json::from_slice(request)?;
    let mut effects = TransferChunkEffects {
        recording_results: VecDeque::from([Err(NativeHostTransferChunkError::ChunkOutOfBounds)]),
        ..TransferChunkEffects::default()
    };

    let responses =
        responses_for_message_with_effects(&mut session, &message, &value, &mut effects);

    assert_eq!(responses.len(), 1);
    assert_eq!(responses[0]["type"], "app.operation.failed");
    assert_eq!(responses[0]["operation"], "recording");
    assert_eq!(responses[0]["operationId"], "recording-1");
    assert_eq!(responses[0]["phase"], "transfer-index");
    assert_eq!(responses[0]["error"]["code"], "chunk-out-of-bounds");
    assert_eq!(responses[0]["error"]["recoverable"], true);
    assert!(responses[0].get("base64").is_none());
    Ok(())
}

#[test]
fn native_host_accepts_recording_ack_for_pending_transfer() -> Result<(), Box<dyn Error>> {
    let mut session = connected_native_host_session()?;
    let ack = br#"{
        "type": "extension.recording.ack",
        "protocolVersion": 1,
        "controllerLeaseId": "lease:connection-1",
        "recordingId": "recording-1"
    }"#;
    let message = parse_wire_message(ack)?;
    let value: Value = serde_json::from_slice(ack)?;
    let mut effects = RecordingTerminalEffects {
        ack_result: Ok(true),
        ..RecordingTerminalEffects::default()
    };

    let responses =
        responses_for_message_with_effects(&mut session, &message, &value, &mut effects);

    assert!(responses.is_empty());
    assert_eq!(effects.acked_recording_id.as_deref(), Some("recording-1"));
    Ok(())
}

#[test]
fn native_host_accepts_screenshot_ack_for_pending_transfer() -> Result<(), Box<dyn Error>> {
    let mut session = connected_native_host_session()?;
    let ack = br#"{
        "type": "extension.screenshot.ack",
        "protocolVersion": 1,
        "controllerLeaseId": "lease:connection-1",
        "captureId": "capture-1",
        "assetId": "asset-1"
    }"#;
    let message = parse_wire_message(ack)?;
    let value: Value = serde_json::from_slice(ack)?;
    let mut effects = ScreenshotTerminalEffects {
        ack_result: Ok(true),
        ..ScreenshotTerminalEffects::default()
    };

    let responses =
        responses_for_message_with_effects(&mut session, &message, &value, &mut effects);

    assert!(responses.is_empty());
    assert_eq!(effects.acked_capture_id.as_deref(), Some("capture-1"));
    Ok(())
}

#[test]
fn native_host_accepts_screenshot_reject_with_diagnostic_retention() -> Result<(), Box<dyn Error>> {
    let mut session = connected_native_host_session()?;
    let reject = br#"{
        "type": "extension.screenshot.reject",
        "protocolVersion": 1,
        "controllerLeaseId": "lease:connection-1",
        "captureId": "capture-1",
        "reason": "user-cancelled",
        "diagnosticRetentionRequested": true
    }"#;
    let message = parse_wire_message(reject)?;
    let value: Value = serde_json::from_slice(reject)?;
    let mut effects = ScreenshotTerminalEffects {
        reject_result: Ok(true),
        ..ScreenshotTerminalEffects::default()
    };

    let responses =
        responses_for_message_with_effects(&mut session, &message, &value, &mut effects);

    assert!(responses.is_empty());
    assert_eq!(effects.rejected_capture_id.as_deref(), Some("capture-1"));
    assert_eq!(effects.retain_for_diagnostics, Some(true));
    Ok(())
}

#[test]
fn native_host_accepts_recording_reject_with_diagnostic_retention() -> Result<(), Box<dyn Error>> {
    let mut session = connected_native_host_session()?;
    let reject = br#"{
        "type": "extension.recording.reject",
        "protocolVersion": 1,
        "controllerLeaseId": "lease:connection-1",
        "recordingId": "recording-1",
        "reason": "user-cancelled",
        "diagnosticRetentionRequested": true
    }"#;
    let message = parse_wire_message(reject)?;
    let value: Value = serde_json::from_slice(reject)?;
    let mut effects = RecordingTerminalEffects {
        reject_result: Ok(true),
        ..RecordingTerminalEffects::default()
    };

    let responses =
        responses_for_message_with_effects(&mut session, &message, &value, &mut effects);

    assert!(responses.is_empty());
    assert_eq!(
        effects.rejected_recording_id.as_deref(),
        Some("recording-1")
    );
    assert_eq!(effects.retain_for_diagnostics, Some(true));
    Ok(())
}

#[test]
fn native_host_reports_storage_failure_when_recording_ack_state_fails() -> Result<(), Box<dyn Error>>
{
    let mut session = connected_native_host_session()?;
    let ack = br#"{
        "type": "extension.recording.ack",
        "protocolVersion": 1,
        "controllerLeaseId": "lease:connection-1",
        "recordingId": "recording-1"
    }"#;
    let message = parse_wire_message(ack)?;
    let value: Value = serde_json::from_slice(ack)?;
    let mut effects = RecordingTerminalEffects {
        ack_result: Err(NativeHostSideEffectError::RecordingTransferState),
        ..RecordingTerminalEffects::default()
    };

    let responses =
        responses_for_message_with_effects(&mut session, &message, &value, &mut effects);

    assert_eq!(responses.len(), 1);
    assert_eq!(responses[0]["type"], "app.operation.failed");
    assert_eq!(responses[0]["operationId"], "recording-1");
    assert_eq!(responses[0]["phase"], "transfer-state");
    assert_eq!(responses[0]["error"]["code"], "storage-failed");
    Ok(())
}
