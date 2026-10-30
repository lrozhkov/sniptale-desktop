use super::*;

mod connection;

fn startup_inputs(repair: bool, cache_supported: bool) -> AgentStartupInputs {
    AgentStartupInputs {
        install_repair_required: repair,
        cache_schema_supported: cache_supported,
        active_settings: crate::settings::ActiveSettings::for_test_mode(
            crate::settings::OperatingMode::ExtensionManaged,
        ),
    }
}

#[test]
fn connected_open_settings_tray_action_requests_extension_navigation() -> Result<(), Box<dyn Error>>
{
    let session = connected_native_host_session()?;
    let request = tray_request(
        "open-settings",
        "invocation-1",
        TrayActionKind::OpenSettings,
    );

    let values = responses_for_tray_action(&session, &request);

    assert_eq!(values.len(), 1);
    assert_eq!(values[0]["type"], "app.openSettings.requested");
    assert_eq!(values[0]["controllerLeaseId"], "lease:connection-1");
    assert_eq!(values[0]["section"], "native-app");
    assert_eq!(values[0]["invocationId"], "invocation-1");
    parse_wire_message(&serde_json::to_vec(&values[0])?)?;
    Ok(())
}

#[test]
fn connected_configured_tray_action_requests_extension_authorization() -> Result<(), Box<dyn Error>>
{
    let session = connected_native_host_session()?;
    let request = tray_request(
        "screenshot-screen",
        "invocation-2",
        TrayActionKind::CaptureScreenshot,
    );

    let values = responses_for_tray_action(&session, &request);

    assert_eq!(values.len(), 1);
    assert_eq!(values[0]["type"], "app.tray.actionRequested");
    assert_eq!(values[0]["controllerLeaseId"], "lease:connection-1");
    assert_eq!(values[0]["actionId"], "screenshot-screen");
    assert_eq!(values[0]["invocationId"], "invocation-2");
    parse_wire_message(&serde_json::to_vec(&values[0])?)?;
    Ok(())
}

#[test]
fn accepted_settings_sync_updates_synced_tray_registry_for_clicks() -> Result<(), Box<dyn Error>> {
    let mut session = connected_native_host_session()?;
    let message = parse_wire_message(extension_settings_sync_with_tray_action())?;
    let value: Value = serde_json::from_slice(extension_settings_sync_with_tray_action())?;

    let accepted = responses_for_message(&mut session, &message, &value);
    assert_eq!(accepted[0]["type"], "app.settings.accepted");

    let values = responses_for_synced_tray_action(
        &session,
        "screenshot-screen",
        true,
        "invocation-5",
        5_000,
    )?;

    assert_eq!(values.len(), 1);
    assert_eq!(values[0]["type"], "app.tray.actionRequested");
    assert_eq!(values[0]["actionId"], "screenshot-screen");
    assert_eq!(values[0]["invocationId"], "invocation-5");
    parse_wire_message(&serde_json::to_vec(&values[0])?)?;
    Ok(())
}

#[test]
fn invalid_tray_registry_settings_sync_fails_without_replacing_previous_registry()
-> Result<(), Box<dyn Error>> {
    let mut session = connected_native_host_session()?;
    let message = parse_wire_message(extension_settings_sync_with_tray_action())?;
    let value: Value = serde_json::from_slice(extension_settings_sync_with_tray_action())?;
    responses_for_message(&mut session, &message, &value);

    let bad_message = parse_wire_message(extension_settings_sync_with_duplicate_tray_action())?;
    let bad_value: Value =
        serde_json::from_slice(extension_settings_sync_with_duplicate_tray_action())?;
    let failed = responses_for_message(&mut session, &bad_message, &bad_value);

    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0]["type"], "app.operation.failed");
    assert_eq!(failed[0]["operation"], "settings-sync");
    assert_eq!(failed[0]["phase"], "tray-actions");
    assert_eq!(failed[0]["error"]["code"], "incompatible-settings");

    let values = responses_for_synced_tray_action(
        &session,
        "screenshot-screen",
        true,
        "invocation-6",
        6_000,
    )?;
    assert_eq!(values[0]["type"], "app.tray.actionRequested");
    assert_eq!(values[0]["invocationId"], "invocation-6");
    Ok(())
}

#[test]
fn disconnected_or_native_only_tray_actions_do_not_emit_extension_requests()
-> Result<(), Box<dyn Error>> {
    let disconnected = NativeHostSession::default();
    let request = tray_request(
        "screenshot-screen",
        "invocation-3",
        TrayActionKind::CaptureScreenshot,
    );
    assert_eq!(responses_for_tray_action(&disconnected, &request).len(), 0);

    let connected = connected_native_host_session()?;
    let quit = tray_request("quit", "invocation-4", TrayActionKind::Quit);
    assert_eq!(responses_for_tray_action(&connected, &quit).len(), 0);
    Ok(())
}

#[test]
fn agent_start_plan_includes_required_resident_owners() {
    let inputs = startup_inputs(false, true);
    let plan = plan_agent_start(&inputs);

    assert_eq!(plan.status, AgentStartupStatus::Operational);
    assert_eq!(
        plan.components,
        vec![
            AgentComponent::Tray,
            AgentComponent::AssetPublisher,
            AgentComponent::Hotkeys,
            AgentComponent::CaptureWorker,
            AgentComponent::TelemetryWorker,
            AgentComponent::EncoderWorker,
            AgentComponent::AudioWorker,
            AgentComponent::TransferWorker,
            AgentComponent::PipeServer,
        ]
    );
}

#[test]
fn agent_start_keeps_pipe_available_when_repair_or_cache_blocks_operations() {
    for (startup, status) in [
        (
            startup_inputs(true, true),
            AgentStartupStatus::RepairRequired,
        ),
        (
            startup_inputs(false, false),
            AgentStartupStatus::UnsupportedCacheSchema,
        ),
        (
            startup_inputs(true, false),
            AgentStartupStatus::RepairRequiredAndUnsupportedCacheSchema,
        ),
    ] {
        let plan = plan_agent_start(&startup);
        assert_eq!(plan.status, status);
        assert_eq!(
            plan.components,
            vec![AgentComponent::Tray, AgentComponent::PipeServer]
        );
        for blocked in [
            AgentComponent::Hotkeys,
            AgentComponent::CaptureWorker,
            AgentComponent::EncoderWorker,
            AgentComponent::AudioWorker,
            AgentComponent::TransferWorker,
            AgentComponent::TelemetryWorker,
        ] {
            assert!(!plan.components.contains(&blocked));
        }
    }
}

#[test]
fn resident_agent_runtime_starts_workers_and_stops_in_reverse_order() -> Result<(), Box<dyn Error>>
{
    let (shutdown_sender, shutdown) = mpsc::sync_channel(1);
    shutdown_sender.send(AgentShutdownSignal::Stop)?;

    let inputs = AgentRunInputs {
        startup: startup_inputs(false, true),
        shutdown,
    };
    let report = run_resident_agent_until_shutdown(&inputs)
        .map_err(|error| IoError::other(format!("{error:?}")))?;

    assert_eq!(report.startup_status, AgentStartupStatus::Operational);
    assert_eq!(
        report.started_components,
        vec![
            AgentComponent::Tray,
            AgentComponent::AssetPublisher,
            AgentComponent::Hotkeys,
            AgentComponent::CaptureWorker,
            AgentComponent::TelemetryWorker,
            AgentComponent::EncoderWorker,
            AgentComponent::AudioWorker,
            AgentComponent::TransferWorker,
            AgentComponent::PipeServer,
        ]
    );
    assert_eq!(
        report.stopped_components,
        vec![
            AgentComponent::PipeServer,
            AgentComponent::TransferWorker,
            AgentComponent::AudioWorker,
            AgentComponent::EncoderWorker,
            AgentComponent::TelemetryWorker,
            AgentComponent::CaptureWorker,
            AgentComponent::Hotkeys,
            AgentComponent::AssetPublisher,
            AgentComponent::Tray,
        ]
    );
    Ok(())
}

#[test]
fn native_host_validates_origin_before_bounded_pipe_startup() -> Result<(), BridgeStartupError> {
    let plan = plan_bridge_startup(BridgeStartupInputs {
        extension_origin_allowed: true,
        agent_running: false,
        max_pipe_connect_attempts: 3,
        pipe_ready_after_attempt: Some(2),
    })?;

    assert!(plan.start_agent_once);
    assert_eq!(plan.pipe_connect_attempts, 2);
    assert_eq!(
        plan_bridge_startup(BridgeStartupInputs {
            extension_origin_allowed: false,
            agent_running: true,
            max_pipe_connect_attempts: 3,
            pipe_ready_after_attempt: Some(1),
        }),
        Err(BridgeStartupError::OriginRejected)
    );
    Ok(())
}

#[test]
fn native_host_startup_fails_when_pipe_never_becomes_ready() {
    assert_eq!(
        plan_bridge_startup(BridgeStartupInputs {
            extension_origin_allowed: true,
            agent_running: true,
            max_pipe_connect_attempts: 3,
            pipe_ready_after_attempt: Some(4),
        }),
        Err(BridgeStartupError::PipeUnavailable)
    );
}

#[test]
fn native_host_runtime_connects_without_spawning_when_pipe_is_ready()
-> Result<(), BridgeStartupError> {
    let starts = Rc::new(RefCell::new(0_u8));
    let connected = connect_pipe_with_bounded_agent_startup(
        3,
        || Ok("pipe-ready"),
        || {
            *starts.borrow_mut() += 1;
            Ok(())
        },
        || {},
    )?;

    assert_eq!(connected.pipe, "pipe-ready");
    assert_eq!(connected.attempts, 1);
    assert!(!connected.started_agent);
    assert_eq!(*starts.borrow(), 0);
    Ok(())
}

#[test]
fn native_host_runtime_starts_agent_once_and_retries_pipe() -> Result<(), BridgeStartupError> {
    let attempts = Rc::new(RefCell::new(0_u8));
    let starts = Rc::new(RefCell::new(0_u8));
    let waits = Rc::new(RefCell::new(0_u8));

    let connected = connect_pipe_with_bounded_agent_startup(
        4,
        {
            let attempts = Rc::clone(&attempts);
            move || {
                *attempts.borrow_mut() += 1;
                if *attempts.borrow() == 3 {
                    Ok("pipe-after-start")
                } else {
                    Err(())
                }
            }
        },
        {
            let starts = Rc::clone(&starts);
            move || {
                *starts.borrow_mut() += 1;
                Ok(())
            }
        },
        {
            let waits = Rc::clone(&waits);
            move || *waits.borrow_mut() += 1
        },
    )?;

    assert_eq!(connected.pipe, "pipe-after-start");
    assert_eq!(connected.attempts, 3);
    assert!(connected.started_agent);
    assert_eq!(*starts.borrow(), 1);
    assert_eq!(*waits.borrow(), 2);
    Ok(())
}

#[test]
fn pipe_bridge_startup_latency_budget_is_attempt_bounded() -> Result<(), BridgeStartupError> {
    let attempts = Rc::new(RefCell::new(0_u8));
    let waits = Rc::new(RefCell::new(0_u8));
    let connected = connect_pipe_with_bounded_agent_startup(
        4,
        {
            let attempts = Rc::clone(&attempts);
            move || {
                *attempts.borrow_mut() += 1;
                (*attempts.borrow() == 2).then_some("pipe").ok_or(())
            }
        },
        || Ok(()),
        {
            let waits = Rc::clone(&waits);
            move || *waits.borrow_mut() += 1
        },
    )?;

    assert_eq!(connected.attempts, 2);
    assert_eq!(*attempts.borrow(), 2);
    assert_eq!(*waits.borrow(), 1);
    Ok(())
}

#[test]
fn native_host_runtime_fails_after_bounded_pipe_retries() {
    let attempts = Rc::new(RefCell::new(0_u8));
    let starts = Rc::new(RefCell::new(0_u8));
    let result = connect_pipe_with_bounded_agent_startup(
        2,
        {
            let attempts = Rc::clone(&attempts);
            move || {
                *attempts.borrow_mut() += 1;
                Err::<(), ()>(())
            }
        },
        {
            let starts = Rc::clone(&starts);
            move || {
                *starts.borrow_mut() += 1;
                Ok(())
            }
        },
        || {},
    );

    assert_eq!(result, Err(BridgeStartupError::PipeUnavailable));
    assert_eq!(*attempts.borrow(), 2);
    assert_eq!(*starts.borrow(), 1);
    assert_eq!(
        connect_pipe_with_bounded_agent_startup(0, || Ok(()), || Ok(()), || {}),
        Err(BridgeStartupError::InvalidAttemptLimit)
    );
}

#[test]
fn native_host_typed_startup_starts_resident_only_for_absent_pipe() -> Result<(), BridgeStartupError>
{
    #[derive(Clone, Copy)]
    enum ConnectProbe {
        NotFound,
    }

    let attempts = Rc::new(RefCell::new(0_u8));
    let starts = Rc::new(RefCell::new(0_u8));
    let connected = connect_pipe_with_typed_agent_startup(
        3,
        {
            let attempts = Rc::clone(&attempts);
            move || {
                *attempts.borrow_mut() += 1;
                if *attempts.borrow() == 2 {
                    Ok("pipe-after-start")
                } else {
                    Err(ConnectProbe::NotFound)
                }
            }
        },
        {
            let starts = Rc::clone(&starts);
            move || {
                *starts.borrow_mut() += 1;
                Ok(())
            }
        },
        || {},
        |error| matches!(error, ConnectProbe::NotFound),
    )?;

    assert_eq!(connected.pipe, "pipe-after-start");
    assert!(connected.started_agent);
    assert_eq!(*attempts.borrow(), 2);
    assert_eq!(*starts.borrow(), 1);
    Ok(())
}

#[test]
fn native_host_typed_startup_does_not_spawn_resident_for_busy_pipe() {
    #[derive(Clone, Copy)]
    enum ConnectProbe {
        Busy,
    }

    let attempts = Rc::new(RefCell::new(0_u8));
    let starts = Rc::new(RefCell::new(0_u8));
    let result = connect_pipe_with_typed_agent_startup(
        3,
        {
            let attempts = Rc::clone(&attempts);
            move || {
                *attempts.borrow_mut() += 1;
                Err::<(), _>(ConnectProbe::Busy)
            }
        },
        {
            let starts = Rc::clone(&starts);
            move || {
                *starts.borrow_mut() += 1;
                Ok(())
            }
        },
        || {},
        |_| false,
    );

    assert_eq!(result, Err(BridgeStartupError::PipeBusy));
    assert_eq!(*attempts.borrow(), 3);
    assert_eq!(*starts.borrow(), 0);
}

#[test]
fn native_host_authenticated_pipe_bridge_handshakes_before_forwarding() -> Result<(), Box<dyn Error>>
{
    let mut agent_input = frame_json(&pipe_challenge())?;
    agent_input.extend(framed_agent_responses(&[app_operation_failed()])?);
    let mut agent_pipe = DuplexPipe::new(agent_input);
    let mut browser_reader = Cursor::new(framed(extension_hello())?);
    let mut browser_writer = Vec::new();

    run_authenticated_pipe_forwarding_bridge(
        &mut browser_reader,
        &mut browser_writer,
        &mut agent_pipe,
        &NativeHostPipeAuthConfig {
            current_user_sid: "S-1-5-21-1000".to_owned(),
            install_id: "install-stable".to_owned(),
            extension_origin: ALLOWED_NATIVE_ORIGIN.to_owned(),
            client_nonce: "client-nonce-0001".to_owned(),
            secret: pipe_secret()?,
        },
    )?;

    let agent_values = read_framed_json_values(&agent_pipe.written)?;
    assert_eq!(agent_values.len(), 3);
    assert_eq!(agent_values[0]["currentUserSid"], "S-1-5-21-1000");
    assert_eq!(agent_values[0]["installId"], "install-stable");
    assert_eq!(agent_values[1]["sessionId"], "session-1");
    assert_eq!(agent_values[1]["serverNonce"], "server-nonce-0001");
    assert_eq!(agent_values[2]["type"], "extension.hello");

    let browser_value = read_framed_json(&browser_writer)?;
    assert_eq!(browser_value["type"], "app.operation.failed");
    assert_eq!(browser_value["operation"], "screenshot");
    Ok(())
}

#[test]
fn shutdown_sequence_orders_ingress_stop_before_active_work_cleanup() {
    let sequence = shutdown_sequence(&ShutdownInputs {
        active_work: vec![
            ActiveWork::Capture,
            ActiveWork::Recording,
            ActiveWork::Transfer,
            ActiveWork::Telemetry,
        ],
    });

    assert_eq!(sequence[0], ShutdownStep::StopAcceptingPipeClients);
    assert_eq!(sequence[1], ShutdownStep::DisableHotkeys);
    assert_eq!(sequence[2], ShutdownStep::HideTray);
    assert!(sequence.contains(&ShutdownStep::CancelCapture));
    assert!(sequence.contains(&ShutdownStep::StopRecording));
    assert!(sequence.contains(&ShutdownStep::FlushTransfer));
    assert!(sequence.contains(&ShutdownStep::StopTelemetry));
    assert_eq!(sequence.last(), Some(&ShutdownStep::PersistTechnicalState));
}

#[test]
fn resident_agent_pipe_session_authenticates_then_handles_forwarded_hello()
-> Result<(), Box<dyn Error>> {
    let challenge = pipe_challenge();
    let response = build_auth_response(&challenge, &pipe_secret()?)?;
    let mut input = frame_json(&bridge_hello())?;
    input.extend(frame_json(&response)?);
    input.extend(framed(extension_hello())?);
    let mut pipe = DuplexPipe::new(input);
    let mut auth = pipe_auth(2, 2)?;

    run_agent_pipe_session(&mut pipe, &mut auth, &pipe_session_config())?;

    let values = read_framed_json_values(&pipe.written)?;
    assert_eq!(values[0]["sessionId"], "session-1");
    assert_eq!(values[1]["responseCount"], 1);
    assert_eq!(values[2]["type"], "app.hello");
    Ok(())
}

#[test]
fn resident_agent_pipe_session_batches_multiple_responses_for_one_forwarded_payload()
-> Result<(), Box<dyn Error>> {
    let challenge = pipe_challenge();
    let response = build_auth_response(&challenge, &pipe_secret()?)?;
    let mut input = frame_json(&bridge_hello())?;
    input.extend(frame_json(&response)?);
    input.extend(framed(extension_hello())?);
    input.extend(framed(extension_controller_acquire())?);
    let mut pipe = DuplexPipe::new(input);
    let mut auth = pipe_auth(2, 2)?;
    let mut effects = PendingRecordingEffects {
        pending: vec![pending_recording_transfer()],
    };

    run_agent_pipe_session_with_effects(
        &mut pipe,
        &mut auth,
        &pipe_session_config(),
        &mut effects,
    )?;

    let values = read_framed_json_values(&pipe.written)?;
    assert_eq!(values[0]["sessionId"], "session-1");
    assert_eq!(values[1]["responseCount"], 1);
    assert_eq!(values[2]["type"], "app.hello");
    assert_eq!(values[3]["responseCount"], 2);
    assert_eq!(values[4]["type"], "app.controller.lease");
    assert_eq!(values[5]["type"], "app.recording.stopped");
    assert_eq!(values[5]["recordingId"], "recording-1");
    Ok(())
}

#[test]
fn resident_pipe_server_releases_controller_lease_after_session_close() -> Result<(), Box<dyn Error>>
{
    let first_written = Rc::new(RefCell::new(Vec::new()));
    let second_written = Rc::new(RefCell::new(Vec::new()));
    let second_acquire = extension_controller_acquire_with_connection("connection-2");
    let mut listener = VecPipeListener::new(vec![
        SharedDuplexPipe::new(
            pipe_server_session_input_many(
                "session-1",
                "server-nonce-0001",
                &[extension_hello(), extension_controller_acquire()],
            )?,
            Rc::clone(&first_written),
        ),
        SharedDuplexPipe::new(
            pipe_server_session_input_many(
                "session-2",
                "server-nonce-0002",
                &[extension_hello(), second_acquire.as_slice()],
            )?,
            Rc::clone(&second_written),
        ),
    ]);
    let mut auth = pipe_auth(2, 2)?;
    let mut configs = FixedSessionConfigs::new(vec![
        pipe_session_config_with("session-1", "server-nonce-0001"),
        pipe_session_config_with("session-2", "server-nonce-0002"),
    ]);
    let mut effects = AcceptingHotkeyEffects;

    let report = run_agent_pipe_server_with_effects(
        &mut listener,
        &mut auth,
        &mut configs,
        &mut effects,
        |_| AgentPipeServerControl::Continue,
    )?;

    assert_eq!(report.completed, 2);
    let first_values = read_framed_json_values(&first_written.borrow())?;
    assert_eq!(first_values[4]["status"], "granted");
    assert_eq!(
        first_values[4]["controller"]["connectionId"],
        "connection-1"
    );

    let second_values = read_framed_json_values(&second_written.borrow())?;
    assert_eq!(second_values[4]["status"], "granted");
    assert_eq!(
        second_values[4]["controller"]["connectionId"],
        "connection-2"
    );
    assert_eq!(second_values[4]["controllerLeaseId"], "lease:connection-2");
    Ok(())
}

#[test]
fn resident_pipe_server_records_rejected_accept_and_keeps_listening() -> Result<(), Box<dyn Error>>
{
    let written = Rc::new(RefCell::new(Vec::new()));
    let mut listener = ScriptedPipeListener::new(vec![
        ScriptedPipeAccept::Rejected(AgentPipeRejection::new(
            "session-rejected",
            "resident_pipe.peer_identity_rejected.untrusted_browser_parent_root",
            "resident pipe rejected native-host session: untrusted browser parent root",
        )),
        ScriptedPipeAccept::Connected(SharedDuplexPipe::new(
            pipe_server_session_input_many(
                "session-1",
                "server-nonce-0001",
                &[extension_hello(), extension_controller_acquire()],
            )?,
            Rc::clone(&written),
        )),
    ]);
    let mut auth = pipe_auth(2, 2)?;
    let mut configs = FixedSessionConfigs::new(vec![pipe_session_config()]);
    let mut effects = AcceptingHotkeyEffects;

    let report = run_agent_pipe_server_with_effects(
        &mut listener,
        &mut auth,
        &mut configs,
        &mut effects,
        |_| AgentPipeServerControl::Continue,
    )?;

    assert_eq!(report.failed, 1);
    assert_eq!(report.accepted, 1);
    assert_eq!(report.completed, 1);
    let values = read_framed_json_values(&written.borrow())?;
    assert_eq!(values[2]["type"], "app.hello");
    assert_eq!(values[4]["type"], "app.controller.lease");
    Ok(())
}

#[test]
fn resident_agent_pipe_session_fails_settings_sync_when_hotkey_effect_fails()
-> Result<(), Box<dyn Error>> {
    let challenge = pipe_challenge();
    let response = build_auth_response(&challenge, &pipe_secret()?)?;
    let mut input = frame_json(&bridge_hello())?;
    input.extend(frame_json(&response)?);
    input.extend(framed(extension_hello())?);
    input.extend(framed(extension_controller_acquire())?);
    input.extend(framed(extension_settings_sync_with_tray_action())?);
    let mut pipe = DuplexPipe::new(input);
    let mut auth = pipe_auth(2, 2)?;
    let mut effects = FailingHotkeyEffects;

    run_agent_pipe_session_with_effects(
        &mut pipe,
        &mut auth,
        &pipe_session_config(),
        &mut effects,
    )?;

    let values = read_framed_json_values(&pipe.written)?;
    assert_eq!(values[0]["sessionId"], "session-1");
    assert_eq!(values[1]["responseCount"], 1);
    assert_eq!(values[2]["type"], "app.hello");
    assert_eq!(values[3]["responseCount"], 1);
    assert_eq!(values[4]["type"], "app.controller.lease");
    assert_eq!(values[5]["responseCount"], 1);
    assert_eq!(values[6]["type"], "app.operation.failed");
    assert_eq!(values[6]["phase"], "hotkeys-runtime");
    Ok(())
}

#[test]
fn resident_agent_pipe_session_writes_hello_before_browser_eof() -> Result<(), Box<dyn Error>> {
    let challenge = pipe_challenge();
    let response = build_auth_response(&challenge, &pipe_secret()?)?;
    let mut input = frame_json(&bridge_hello())?;
    input.extend(frame_json(&response)?);
    input.extend(framed(extension_hello())?);
    let (release_eof, wait_for_eof) = mpsc::sync_channel(1);
    let written = Arc::new(Mutex::new(Vec::new()));
    let pipe = ThreadedSplitPipe::new(input, Arc::clone(&written), wait_for_eof);
    let mut auth = pipe_auth(1, 8)?;
    let mut effects = AcceptingHotkeyEffects;
    let events = AgentEventHub::default();
    let event_receiver = events.subscribe();

    let session = thread::spawn(move || {
        run_agent_pipe_session_with_effects_and_events(
            pipe,
            &mut auth,
            &pipe_session_config(),
            &mut effects,
            event_receiver,
        )
    });

    wait_for_written_type(&written, "app.hello")?;
    let _ = release_eof.send(());
    session
        .join()
        .map_err(|_| IoError::other("session thread panicked"))??;

    let values = read_framed_json_values(&written.lock().map_err(|_| IoError::other("lock"))?)?;
    assert_eq!(values[1]["responseCount"], 1);
    assert_eq!(values[2]["type"], "app.hello");
    Ok(())
}

#[test]
fn resident_agent_pipe_session_writes_outbound_event_without_next_browser_message()
-> Result<(), Box<dyn Error>> {
    let challenge = pipe_challenge();
    let response = build_auth_response(&challenge, &pipe_secret()?)?;
    let mut input = frame_json(&bridge_hello())?;
    input.extend(frame_json(&response)?);
    input.extend(framed(extension_hello())?);
    input.extend(framed(extension_controller_acquire())?);
    input.extend(framed(extension_settings_sync_with_tray_action())?);
    let (release_eof, wait_for_eof) = mpsc::sync_channel(1);
    let written = Arc::new(Mutex::new(Vec::new()));
    let pipe = ThreadedSplitPipe::new(input, Arc::clone(&written), wait_for_eof);
    let mut auth = pipe_auth(1, 8)?;
    let mut effects = AcceptingHotkeyEffects;
    let events = AgentEventHub::default();
    let event_receiver = events.subscribe();
    let publisher = events.clone();

    let session = thread::spawn(move || {
        run_agent_pipe_session_with_effects_and_events(
            pipe,
            &mut auth,
            &pipe_session_config(),
            &mut effects,
            event_receiver,
        )
    });

    wait_for_written_type(&written, "app.settings.accepted")?;
    assert_eq!(
        publisher.publish_hotkey_action("screenshot-screen"),
        AgentEventPublishResult::Queued
    );
    wait_for_written_type(&written, "app.tray.actionRequested")?;
    let _ = release_eof.send(());
    session
        .join()
        .map_err(|_| IoError::other("session thread panicked"))??;

    let values = read_framed_json_values(&written.lock().map_err(|_| IoError::other("lock"))?)?;
    assert!(values.iter().any(|value| {
        value["type"] == "app.tray.actionRequested" && value["actionId"] == "screenshot-screen"
    }));
    Ok(())
}

#[test]
fn resident_agent_duplex_session_releases_controller_lease_after_session_close()
-> Result<(), Box<dyn Error>> {
    let shared_state = NativeHostSharedState::default();
    let mut auth = pipe_auth(2, 8)?;
    let events = AgentEventHub::default();

    let first_written = run_split_pipe_session_with_state(
        "session-1",
        "server-nonce-0001",
        extension_controller_acquire(),
        shared_state.clone(),
        &mut auth,
        &events,
    )?;
    let first_values = read_framed_json_values(&first_written)?;
    assert_eq!(first_values[4]["status"], "granted");
    assert_eq!(
        first_values[4]["controller"]["connectionId"],
        "connection-1"
    );

    let second_written = run_split_pipe_session_with_state(
        "session-2",
        "server-nonce-0002",
        extension_controller_acquire_with_connection("connection-2"),
        shared_state,
        &mut auth,
        &events,
    )?;
    let second_values = read_framed_json_values(&second_written)?;
    assert_eq!(second_values[4]["status"], "granted");
    assert_eq!(
        second_values[4]["controller"]["connectionId"],
        "connection-2"
    );
    assert_eq!(second_values[4]["controllerLeaseId"], "lease:connection-2");
    Ok(())
}

#[test]
fn resident_agent_duplex_session_treats_closed_transport_as_clean_close()
-> Result<(), Box<dyn Error>> {
    let challenge = pipe_challenge();
    let response = build_auth_response(&challenge, &pipe_secret()?)?;
    let mut input = frame_json(&bridge_hello())?;
    input.extend(frame_json(&response)?);
    input.extend(framed(extension_hello())?);
    let (release_eof, wait_for_eof) = mpsc::sync_channel(1);
    let written = Arc::new(Mutex::new(Vec::new()));
    let pipe = ThreadedSplitPipe::new_failing_writer_after_writes(
        input,
        Arc::clone(&written),
        wait_for_eof,
        ErrorKind::BrokenPipe,
        2,
    );
    let mut auth = pipe_auth(1, 2)?;
    let mut effects = AcceptingHotkeyEffects;
    let events = AgentEventHub::default();
    let event_receiver = events.subscribe();

    let _ = release_eof.send(());
    run_agent_pipe_session_with_effects_and_events(
        pipe,
        &mut auth,
        &pipe_session_config(),
        &mut effects,
        event_receiver,
    )?;

    let challenge = pipe_challenge_with("session-2", "server-nonce-0002");
    let response = build_auth_response(&challenge, &pipe_secret()?)?;
    let mut input = frame_json(&bridge_hello())?;
    input.extend(frame_json(&response)?);
    input.extend(framed(extension_hello())?);
    let mut pipe = DuplexPipe::new(input);

    run_agent_pipe_session(
        &mut pipe,
        &mut auth,
        &pipe_session_config_with("session-2", "server-nonce-0002"),
    )?;
    let values = read_framed_json_values(&pipe.written)?;
    assert_eq!(values[0]["sessionId"], "session-2");
    assert_eq!(values[2]["type"], "app.hello");
    Ok(())
}

#[test]
fn resident_agent_duplex_session_treats_reader_broken_pipe_as_clean_close()
-> Result<(), Box<dyn Error>> {
    let challenge = pipe_challenge();
    let response = build_auth_response(&challenge, &pipe_secret()?)?;
    let mut input = frame_json(&bridge_hello())?;
    input.extend(frame_json(&response)?);
    input.extend(framed(extension_hello())?);
    let (release_eof, wait_for_eof) = mpsc::sync_channel(1);
    let written = Arc::new(Mutex::new(Vec::new()));
    let pipe = ThreadedSplitPipe::new_failing_reader(
        input,
        Arc::clone(&written),
        wait_for_eof,
        ErrorKind::BrokenPipe,
    );
    let mut auth = pipe_auth(1, 2)?;
    let mut effects = AcceptingHotkeyEffects;
    let events = AgentEventHub::default();
    let event_receiver = events.subscribe();

    let _ = release_eof.send(());
    run_agent_pipe_session_with_effects_and_events(
        pipe,
        &mut auth,
        &pipe_session_config(),
        &mut effects,
        event_receiver,
    )?;

    let values = read_framed_json_values(&written.lock().map_err(|_| IoError::other("lock"))?)?;
    assert_eq!(values[0]["sessionId"], "session-1");
    assert_eq!(values[2]["type"], "app.hello");
    Ok(())
}

#[test]
fn resident_agent_pipe_session_rejects_malformed_forwarded_payload_after_auth()
-> Result<(), Box<dyn Error>> {
    let challenge = pipe_challenge();
    let response = build_auth_response(&challenge, &pipe_secret()?)?;
    let mut input = frame_json(&bridge_hello())?;
    input.extend(frame_json(&response)?);
    input.extend(framed(br#"{"type":"extension.hello""#)?);
    let mut pipe = DuplexPipe::new(input);
    let mut auth = pipe_auth(1, 2)?;

    let result = run_agent_pipe_session(&mut pipe, &mut auth, &pipe_session_config());

    assert!(matches!(result, Err(AgentPipeSessionError::Protocol(_))));
    let values = read_framed_json_values(&pipe.written)?;
    assert_eq!(values.len(), 1);
    assert_eq!(values[0]["sessionId"], "session-1");

    let challenge = pipe_challenge_with("session-2", "server-nonce-0002");
    let response = build_auth_response(&challenge, &pipe_secret()?)?;
    let mut input = frame_json(&bridge_hello())?;
    input.extend(frame_json(&response)?);
    input.extend(framed(extension_hello())?);
    let mut pipe = DuplexPipe::new(input);

    run_agent_pipe_session(
        &mut pipe,
        &mut auth,
        &pipe_session_config_with("session-2", "server-nonce-0002"),
    )?;
    let values = read_framed_json_values(&pipe.written)?;
    assert_eq!(values[0]["sessionId"], "session-2");
    assert_eq!(values[1]["responseCount"], 1);
    assert_eq!(values[2]["type"], "app.hello");
    Ok(())
}

#[test]
fn pipe_server_continues_after_malformed_authenticated_session() -> Result<(), Box<dyn Error>> {
    let first_written = Rc::new(RefCell::new(Vec::new()));
    let second_written = Rc::new(RefCell::new(Vec::new()));
    let mut listener = VecPipeListener::new(vec![
        SharedDuplexPipe::new(
            pipe_server_session_input("session-1", "server-nonce-0001", extension_hello())?,
            Rc::clone(&first_written),
        ),
        SharedDuplexPipe::new(
            pipe_server_session_input(
                "session-2",
                "server-nonce-0002",
                br#"{"type":"extension.hello""#,
            )?,
            Rc::clone(&second_written),
        ),
    ]);
    let mut auth = pipe_auth(1, 2)?;
    let mut configs = FixedSessionConfigs::new(vec![
        pipe_session_config_with("session-1", "server-nonce-0001"),
        pipe_session_config_with("session-2", "server-nonce-0002"),
    ]);

    let report = run_agent_pipe_server(&mut listener, &mut auth, &mut configs, |_| {
        AgentPipeServerControl::Continue
    })?;

    assert_eq!(
        report,
        AgentPipeServerReport {
            accepted: 2,
            completed: 1,
            failed: 1,
            startup_failures: 0,
        }
    );
    let first = read_framed_json_values(&first_written.borrow())?;
    assert_eq!(first[0]["sessionId"], "session-1");
    assert_eq!(first[1]["responseCount"], 1);
    assert_eq!(first[2]["type"], "app.hello");
    let second = read_framed_json_values(&second_written.borrow())?;
    assert_eq!(second.len(), 1);
    assert_eq!(second[0]["sessionId"], "session-2");
    Ok(())
}

#[test]
fn pipe_server_stops_before_accept_when_control_requests_stop() -> Result<(), Box<dyn Error>> {
    let mut listener = VecPipeListener::new(vec![SharedDuplexPipe::new(
        pipe_server_session_input("session-1", "server-nonce-0001", extension_hello())?,
        Rc::new(RefCell::new(Vec::new())),
    )]);
    let mut auth = pipe_auth(1, 2)?;
    let mut configs = FixedSessionConfigs::new(vec![pipe_session_config_with(
        "session-1",
        "server-nonce-0001",
    )]);

    let report = run_agent_pipe_server(&mut listener, &mut auth, &mut configs, |_| {
        AgentPipeServerControl::Stop
    })?;

    assert_eq!(report, AgentPipeServerReport::default());
    assert_eq!(listener.remaining(), 1);
    Ok(())
}

#[test]
fn pipe_server_pending_accept_does_not_start_session() -> Result<(), Box<dyn Error>> {
    let mut listener = ScriptedPipeListener::new(vec![
        ScriptedPipeAccept::Pending,
        ScriptedPipeAccept::Connected(SharedDuplexPipe::new(
            pipe_server_session_input("session-1", "server-nonce-0001", extension_hello())?,
            Rc::new(RefCell::new(Vec::new())),
        )),
    ]);
    let mut auth = pipe_auth(1, 2)?;
    let mut configs = FixedSessionConfigs::new(vec![pipe_session_config_with(
        "session-1",
        "server-nonce-0001",
    )]);
    let mut polls = 0;

    let report = run_agent_pipe_server(&mut listener, &mut auth, &mut configs, |_| {
        polls += 1;
        if polls > 1 {
            AgentPipeServerControl::Stop
        } else {
            AgentPipeServerControl::Continue
        }
    })?;

    assert_eq!(report, AgentPipeServerReport::default());
    assert_eq!(listener.remaining(), 1);
    Ok(())
}

#[test]
fn duplex_pipe_server_accepts_next_session_while_first_session_stays_open()
-> Result<(), Box<dyn Error>> {
    let first_written = Arc::new(Mutex::new(Vec::new()));
    let second_written = Arc::new(Mutex::new(Vec::new()));
    let (release_first, wait_first) = mpsc::sync_channel(1);
    let (release_second, wait_second) = mpsc::sync_channel(1);
    let listener = ThreadedPipeListener::new(vec![
        ThreadedSplitPipe::new(
            pipe_server_session_input("session-1", "server-nonce-0001", extension_hello())?,
            Arc::clone(&first_written),
            wait_first,
        ),
        ThreadedSplitPipe::new(
            pipe_server_session_input("session-2", "server-nonce-0002", extension_hello())?,
            Arc::clone(&second_written),
            wait_second,
        ),
    ]);
    let auth = pipe_auth(2, 2)?;
    let configs = FixedSessionConfigs::new(vec![
        pipe_session_config_with("session-1", "server-nonce-0001"),
        pipe_session_config_with("session-2", "server-nonce-0002"),
    ]);
    let events = AgentEventHub::default();
    let _ = release_second.send(());

    let server = thread::spawn(move || {
        let mut listener = listener;
        let mut auth = auth;
        let mut configs = configs;
        let effects_factory = || AcceptingHotkeyEffects;
        run_agent_pipe_server_with_effects_and_events(
            &mut listener,
            &mut auth,
            &mut configs,
            &effects_factory,
            &events,
            |report| {
                if report.completed + report.failed >= 2 {
                    AgentPipeServerControl::Stop
                } else {
                    AgentPipeServerControl::Continue
                }
            },
        )
    });

    wait_for_written_type(&first_written, "app.hello")?;
    wait_for_written_type(&second_written, "app.hello")?;
    let _ = release_first.send(());
    let report = server
        .join()
        .map_err(|_| IoError::other("pipe server thread panicked"))??;

    assert_eq!(
        report,
        AgentPipeServerReport {
            accepted: 2,
            completed: 2,
            failed: 0,
            startup_failures: 0,
        }
    );
    Ok(())
}

#[test]
fn duplex_pipe_server_readiness_timeout_does_not_block_next_session() -> Result<(), Box<dyn Error>>
{
    let first_written = Arc::new(Mutex::new(Vec::new()));
    let second_written = Arc::new(Mutex::new(Vec::new()));
    let (release_first_eof, wait_first_eof) = mpsc::sync_channel(1);
    let (release_second_eof, wait_second_eof) = mpsc::sync_channel(1);
    let (release_first_split, wait_first_split) = mpsc::sync_channel(1);
    let listener = ThreadedPipeListener::new(vec![
        ThreadedSplitPipe::new(
            pipe_server_session_input("session-1", "server-nonce-0001", extension_hello())?,
            Arc::clone(&first_written),
            wait_first_eof,
        )
        .with_split_gate(wait_first_split),
        ThreadedSplitPipe::new(
            pipe_server_session_input("session-2", "server-nonce-0002", extension_hello())?,
            Arc::clone(&second_written),
            wait_second_eof,
        ),
    ]);
    let stop = Arc::new(AtomicBool::new(false));
    let server_stop = Arc::clone(&stop);
    let _ = release_second_eof.send(());
    let server = thread::spawn(move || {
        let mut listener = listener;
        let mut auth = pipe_auth(2, 2).map_err(|_| AgentPipeServerError::Accept)?;
        let mut configs = FixedSessionConfigs::new(vec![
            pipe_session_config_with("session-1", "server-nonce-0001"),
            pipe_session_config_with("session-2", "server-nonce-0002"),
        ]);
        let events = AgentEventHub::default();
        let effects_factory = || AcceptingHotkeyEffects;
        run_agent_pipe_server_with_effects_and_events(
            &mut listener,
            &mut auth,
            &mut configs,
            &effects_factory,
            &events,
            move |_| {
                if server_stop.load(Ordering::Acquire) {
                    AgentPipeServerControl::Stop
                } else {
                    AgentPipeServerControl::Continue
                }
            },
        )
    });

    wait_for_written_type(&second_written, "app.hello")?;
    let first_output = first_written
        .lock()
        .map_err(|_| IoError::other("lock"))?
        .clone();
    assert_eq!(read_framed_json_values(&first_output)?.len(), 0);
    let _ = release_first_split.send(());
    let _ = release_first_eof.send(());
    stop.store(true, Ordering::Release);
    let report = server
        .join()
        .map_err(|_| IoError::other("pipe server thread panicked"))??;

    assert_eq!(
        report,
        AgentPipeServerReport {
            accepted: 2,
            completed: 2,
            failed: 0,
            startup_failures: 1,
        }
    );
    Ok(())
}

#[test]
fn duplex_pipe_server_does_not_accept_over_active_session_limit() -> Result<(), Box<dyn Error>> {
    let first_written = Arc::new(Mutex::new(Vec::new()));
    let second_written = Arc::new(Mutex::new(Vec::new()));
    let (release_first, wait_first) = mpsc::sync_channel(1);
    let (_release_second, wait_second) = mpsc::sync_channel(1);
    let listener = ThreadedPipeListener::new(vec![
        ThreadedSplitPipe::new(
            pipe_server_session_input("session-1", "server-nonce-0001", extension_hello())?,
            Arc::clone(&first_written),
            wait_first,
        )
        .with_shutdown_sender(release_first.clone()),
        ThreadedSplitPipe::new(
            pipe_server_session_input("session-2", "server-nonce-0002", extension_hello())?,
            Arc::clone(&second_written),
            wait_second,
        ),
    ]);
    let stop = Arc::new(AtomicBool::new(false));
    let server_stop = Arc::clone(&stop);
    let server = thread::spawn(move || {
        let mut listener = listener;
        let mut auth = pipe_auth(1, 2).map_err(|_| AgentPipeServerError::Accept)?;
        let mut configs = FixedSessionConfigs::new(vec![
            pipe_session_config_with("session-1", "server-nonce-0001"),
            pipe_session_config_with("session-2", "server-nonce-0002"),
        ]);
        let events = AgentEventHub::default();
        let effects_factory = || AcceptingHotkeyEffects;
        run_agent_pipe_server_with_effects_and_events(
            &mut listener,
            &mut auth,
            &mut configs,
            &effects_factory,
            &events,
            move |_| {
                if server_stop.load(Ordering::Acquire) {
                    AgentPipeServerControl::Stop
                } else {
                    AgentPipeServerControl::Continue
                }
            },
        )
    });

    wait_for_written_type(&first_written, "app.hello")?;
    thread::sleep(Duration::from_millis(50));
    let second_output = second_written
        .lock()
        .map_err(|_| IoError::other("lock"))?
        .clone();
    assert_eq!(read_framed_json_values(&second_output)?.len(), 0);
    stop.store(true, Ordering::Release);
    let report = server
        .join()
        .map_err(|_| IoError::other("pipe server thread panicked"))??;

    assert_eq!(
        report,
        AgentPipeServerReport {
            accepted: 1,
            completed: 1,
            failed: 0,
            startup_failures: 0,
        }
    );
    Ok(())
}

#[test]
fn duplex_pipe_server_stalled_handshake_does_not_block_next_session_auth()
-> Result<(), Box<dyn Error>> {
    let first_written = Arc::new(Mutex::new(Vec::new()));
    let second_written = Arc::new(Mutex::new(Vec::new()));
    let (release_first, wait_first) = mpsc::sync_channel(1);
    let (release_second, wait_second) = mpsc::sync_channel(1);
    let listener = ThreadedPipeListener::new(vec![
        ThreadedSplitPipe::new(
            pipe_server_hello_only_input()?,
            Arc::clone(&first_written),
            wait_first,
        )
        .with_shutdown_sender(release_first),
        ThreadedSplitPipe::new(
            pipe_server_session_input("session-2", "server-nonce-0002", extension_hello())?,
            Arc::clone(&second_written),
            wait_second,
        )
        .with_shutdown_sender(release_second.clone()),
    ]);
    let _ = release_second.send(());
    let server = thread::spawn(move || {
        let mut listener = listener;
        let mut auth = pipe_auth(2, 2).map_err(|_| AgentPipeServerError::Accept)?;
        let mut configs = FixedSessionConfigs::new(vec![
            pipe_session_config_with("session-1", "server-nonce-0001"),
            pipe_session_config_with("session-2", "server-nonce-0002"),
        ]);
        let events = AgentEventHub::default();
        let effects_factory = || AcceptingHotkeyEffects;
        run_agent_pipe_server_with_effects_and_events(
            &mut listener,
            &mut auth,
            &mut configs,
            &effects_factory,
            &events,
            |report| {
                if report.accepted >= 2 {
                    AgentPipeServerControl::Stop
                } else {
                    AgentPipeServerControl::Continue
                }
            },
        )
    });

    let report = server
        .join()
        .map_err(|_| IoError::other("pipe server thread panicked"))??;
    let second =
        read_framed_json_values(&second_written.lock().map_err(|_| IoError::other("lock"))?)?;
    assert!(second.iter().any(|value| value["type"] == "app.hello"));

    assert_eq!(
        report,
        AgentPipeServerReport {
            accepted: 2,
            completed: 1,
            failed: 1,
            startup_failures: 0,
        }
    );
    Ok(())
}

#[test]
fn duplex_pipe_server_stop_cancels_active_session_read() -> Result<(), Box<dyn Error>> {
    let written = Arc::new(Mutex::new(Vec::new()));
    let (release_read, wait_read) = mpsc::sync_channel(1);
    let listener = ThreadedPipeListener::new(vec![
        ThreadedSplitPipe::new_failing_reader(
            pipe_server_session_input("session-1", "server-nonce-0001", extension_hello())?,
            Arc::clone(&written),
            wait_read,
            ErrorKind::ConnectionAborted,
        )
        .with_shutdown_sender(release_read),
    ]);
    let stop = Arc::new(AtomicBool::new(false));
    let server_stop = Arc::clone(&stop);
    let server = thread::spawn(move || {
        let mut listener = listener;
        let mut auth = pipe_auth(2, 2).map_err(|_| AgentPipeServerError::Accept)?;
        let mut configs = FixedSessionConfigs::new(vec![pipe_session_config_with(
            "session-1",
            "server-nonce-0001",
        )]);
        let events = AgentEventHub::default();
        let effects_factory = || AcceptingHotkeyEffects;
        run_agent_pipe_server_with_effects_and_events(
            &mut listener,
            &mut auth,
            &mut configs,
            &effects_factory,
            &events,
            move |_| {
                if server_stop.load(Ordering::Acquire) {
                    AgentPipeServerControl::Stop
                } else {
                    AgentPipeServerControl::Continue
                }
            },
        )
    });

    wait_for_written_type(&written, "app.hello")?;
    stop.store(true, Ordering::Release);
    let report = server
        .join()
        .map_err(|_| IoError::other("pipe server thread panicked"))??;

    assert_eq!(report.accepted, 1);
    assert_eq!(report.completed, 1);
    assert_eq!(report.failed, 0);
    Ok(())
}

#[test]
fn pipe_session_config_generator_uses_entropy_and_clock() -> Result<(), Box<dyn Error>> {
    let entropy = FixedEntropy::new([[0x11; 16], [0x22; 16], [0x33; 16], [0x44; 16]]);
    let clock = FixedClock::new([10_000, 10_500]);
    let mut generator = AgentPipeSessionConfigGenerator::new(entropy, clock, 5_000);

    let first = generator
        .next_session_config()
        .map_err(|error| IoError::other(format!("{error:?}")))?;
    let second = generator
        .next_session_config()
        .map_err(|error| IoError::other(format!("{error:?}")))?;

    assert_eq!(first.now_ms, 10_000);
    assert_eq!(first.challenge_ttl_ms, 5_000);
    assert_eq!(first.session_id, format!("session-{}", "11".repeat(16)));
    assert_eq!(
        first.server_nonce,
        format!("server-nonce-{}", "22".repeat(16))
    );
    assert_eq!(second.now_ms, 10_500);
    assert_ne!(first.session_id, second.session_id);
    assert_ne!(first.server_nonce, second.server_nonce);
    Ok(())
}
