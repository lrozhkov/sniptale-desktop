use super::*;

#[test]
fn recording_happy_path_reaches_completed_transfer() -> Result<(), RecordingStateError> {
    let mut session = RecordingSession::default();

    assert_eq!(session.start("recording-1")?, TransitionOutcome::Changed);
    assert_eq!(session.state(), RecordingState::Preparing);
    session.begin_recording()?;
    session.apply_control(RecordingControl::Pause)?;
    assert_eq!(session.state(), RecordingState::Paused);
    session.apply_control(RecordingControl::Resume)?;
    session.apply_control(RecordingControl::Stop)?;
    session.mark_finalizing()?;
    session.mark_transferring()?;
    session.complete()?;

    assert_eq!(session.state(), RecordingState::Completed);
    assert_eq!(session.terminal_reason(), Some(TerminalReason::Completed));
    Ok(())
}

#[test]
fn invalid_duplicate_start_is_rejected() -> Result<(), RecordingStateError> {
    let mut session = RecordingSession::default();
    session.start("recording-1")?;

    assert!(matches!(
        session.start("recording-2"),
        Err(RecordingStateError::TransitionRejected {
            from: RecordingState::Preparing,
            action: "start"
        })
    ));
    assert_eq!(session.recording_id(), Some("recording-1"));
    Ok(())
}

#[test]
fn cancel_is_terminal_and_idempotent() -> Result<(), RecordingStateError> {
    let mut session = RecordingSession::default();
    session.start("recording-1")?;
    session.begin_recording()?;
    session.apply_control(RecordingControl::Cancel)?;

    assert_eq!(session.state(), RecordingState::Completed);
    assert_eq!(session.terminal_reason(), Some(TerminalReason::Cancelled));
    assert_eq!(
        session.apply_control(RecordingControl::Cancel)?,
        TransitionOutcome::IgnoredTerminal
    );
    assert_eq!(session.complete()?, TransitionOutcome::IgnoredTerminal);
    Ok(())
}

#[test]
fn failures_are_terminal_and_do_not_become_success() -> Result<(), RecordingStateError> {
    let mut session = RecordingSession::default();
    session.start("recording-1")?;
    session.begin_recording()?;
    session.fail(TerminalReason::Failed)?;

    assert_eq!(session.state(), RecordingState::Failed);
    assert_eq!(session.terminal_reason(), Some(TerminalReason::Failed));
    assert_eq!(session.complete()?, TransitionOutcome::IgnoredTerminal);
    assert_eq!(session.state(), RecordingState::Failed);
    Ok(())
}

#[test]
fn pause_resume_are_state_scoped() -> Result<(), RecordingStateError> {
    let mut session = RecordingSession::default();
    session.start("recording-1")?;

    assert!(matches!(
        session.apply_control(RecordingControl::Pause),
        Err(RecordingStateError::TransitionRejected {
            from: RecordingState::Preparing,
            action: "pause"
        })
    ));
    session.begin_recording()?;
    session.apply_control(RecordingControl::Pause)?;
    assert!(matches!(
        session.apply_control(RecordingControl::Pause),
        Err(RecordingStateError::TransitionRejected {
            from: RecordingState::Paused,
            action: "pause"
        })
    ));
    Ok(())
}

#[test]
fn recording_lifecycle_service_emits_started_and_progress_messages()
-> Result<(), RecordingRuntimeError> {
    let temp = tempfile::tempdir().map_err(|_| RecordingRuntimeError::TransferState)?;
    let mut service = recording_lifecycle_service(temp.path(), FakeRecordingRuntime)?;

    let messages = service.start_recording(
        &recording_start_command(VideoMode::Screen),
        RecordingStartPolicy::default(),
    )?;

    assert_eq!(messages.len(), 2);
    let RecordingCommandEvent::Started(started) = &messages[0] else {
        return Err(RecordingRuntimeError::InvalidState);
    };
    assert_eq!(started.recording_id, COMMAND_1_RECORDING_ID);
    assert_eq!(started.timebase.id, COMMAND_1_RECORDING_TIMEBASE_ID);
    assert_eq!(started.timebase.started_at_epoch_ms, 4_000);
    assert_eq!(started.timebase.started_at_monotonic_ns, "987654321");
    assert_eq!(started.effective_quality.effective_fps.as_u64(), Some(30));
    let bitrate = started
        .effective_quality
        .effective_video_bitrate_mbps
        .as_f64()
        .ok_or(RecordingRuntimeError::InvalidState)?;
    assert!((bitrate - 8.087).abs() < 0.001);
    assert_eq!(
        started.effective_quality.warnings[0].code,
        "unsupported-capability"
    );

    let RecordingCommandEvent::Progress(progress) = &messages[1] else {
        return Err(RecordingRuntimeError::InvalidState);
    };
    assert_eq!(progress.recording_id, COMMAND_1_RECORDING_ID);
    assert_eq!(progress.status, RecordingProgressStatus::Recording);
    Ok(())
}

#[test]
fn recording_lifecycle_starts_manual_route_without_a_fabricated_delivery_session()
-> Result<(), RecordingRuntimeError> {
    let temp = tempfile::tempdir().map_err(|_| RecordingRuntimeError::TransferState)?;
    let telemetry = SharedLifecycleTelemetry::default();
    let mut service = RecordingLifecycleService::new_with_test_inputs_and_telemetry(
        recording_transfer_store(temp.path())?,
        FakeRecordingRuntime,
        telemetry.clone(),
        sufficient_disk_space_probe,
        TestRecordingMonotonicClock::fixed(fixed_monotonic_ns),
    );
    let mut request = recording_start_command(VideoMode::Screen);
    request.authority = crate::protocol::OperationAuthority::Manual {
        profile_revision: "profile-revision-1".to_owned(),
    };
    request.delivery_route = crate::protocol::DeliveryRoute::LocalAsset {
        asset_job_id: "asset-job-1".to_owned(),
    };

    let started = service.start_recording(&request, RecordingStartPolicy::default())?;
    assert!(
        started
            .iter()
            .any(|event| matches!(event, RecordingCommandEvent::Started(_)))
    );

    let mut stop = recording_control_command(
        crate::protocol::RecordingControl::Stop,
        COMMAND_1_RECORDING_ID,
        6_000,
    );
    stop.delivery_session.controller_lease_id = None;
    let completed = service.control_recording(&stop)?;
    assert!(matches!(
        completed.last(),
        Some(RecordingCommandEvent::ManualFinalized(event))
            if event.recording_id == COMMAND_1_RECORDING_ID && event.asset_job_id == "asset-job-1"
    ));
    assert!(
        temp.path()
            .read_dir()
            .map_err(|_| RecordingRuntimeError::TransferState)?
            .next()
            .is_some()
    );
    assert!(telemetry.events().is_empty());
    Ok(())
}

#[test]
fn recording_lifecycle_service_maps_controls_to_progress() -> Result<(), RecordingRuntimeError> {
    let temp = tempfile::tempdir().map_err(|_| RecordingRuntimeError::TransferState)?;
    let mut service = recording_lifecycle_service(temp.path(), FakeRecordingRuntime)?;
    service.start_recording(
        &recording_start_command(VideoMode::ActiveWindow),
        RecordingStartPolicy::default(),
    )?;

    let pause = service.control_recording(&recording_control_command(
        crate::protocol::RecordingControl::Pause,
        COMMAND_1_RECORDING_ID,
        4_500,
    ))?;
    let RecordingCommandEvent::Progress(progress) = &pause[0] else {
        return Err(RecordingRuntimeError::InvalidState);
    };
    assert_eq!(progress.status, RecordingProgressStatus::Paused);
    assert_eq!(progress.duration_ms, 500);

    let resume = service.control_recording(&recording_control_command(
        crate::protocol::RecordingControl::Resume,
        COMMAND_1_RECORDING_ID,
        5_000,
    ))?;
    let RecordingCommandEvent::Progress(progress) = &resume[0] else {
        return Err(RecordingRuntimeError::InvalidState);
    };
    assert_eq!(progress.status, RecordingProgressStatus::Recording);
    assert_eq!(progress.duration_ms, 500);

    let stop = service.control_recording(&recording_control_command(
        crate::protocol::RecordingControl::Stop,
        COMMAND_1_RECORDING_ID,
        6_000,
    ))?;
    let RecordingCommandEvent::Progress(progress) = &stop[0] else {
        return Err(RecordingRuntimeError::InvalidState);
    };
    assert_eq!(progress.status, RecordingProgressStatus::Stopping);
    assert_eq!(progress.duration_ms, 1_500);
    let RecordingCommandEvent::Progress(progress) = &stop[1] else {
        return Err(RecordingRuntimeError::InvalidState);
    };
    assert_eq!(progress.status, RecordingProgressStatus::Finalizing);
    let RecordingCommandEvent::Stopped(stopped) = &stop[2] else {
        return Err(RecordingRuntimeError::InvalidState);
    };
    assert_eq!(stopped.pending.recording_id, COMMAND_1_RECORDING_ID);
    assert_eq!(stopped.pending.duration_ms, 1_500);
    assert_eq!(stopped.pending.total_bytes, 11);
    assert_eq!(stopped.pending.sha256, sha256_hex(b"hello world"));
    Ok(())
}

#[test]
fn recording_lifecycle_service_stops_with_open_pause_interval() -> Result<(), RecordingRuntimeError>
{
    let temp = tempfile::tempdir().map_err(|_| RecordingRuntimeError::TransferState)?;
    let mut service = recording_lifecycle_service(temp.path(), FakeRecordingRuntime)?;
    service.start_recording(
        &recording_start_command(VideoMode::Screen),
        RecordingStartPolicy::default(),
    )?;
    service.control_recording(&recording_control_command(
        crate::protocol::RecordingControl::Pause,
        COMMAND_1_RECORDING_ID,
        4_500,
    ))?;

    let stop = service.control_recording(&recording_control_command(
        crate::protocol::RecordingControl::Stop,
        COMMAND_1_RECORDING_ID,
        6_000,
    ))?;

    let RecordingCommandEvent::Progress(progress) = &stop[0] else {
        return Err(RecordingRuntimeError::InvalidState);
    };
    assert_eq!(progress.status, RecordingProgressStatus::Stopping);
    assert_eq!(progress.duration_ms, 500);
    let RecordingCommandEvent::Stopped(stopped) = &stop[2] else {
        return Err(RecordingRuntimeError::InvalidState);
    };
    assert_eq!(stopped.pending.duration_ms, 500);
    Ok(())
}

#[test]
fn recording_lifecycle_service_rolls_back_pause_when_runtime_rejects_signal()
-> Result<(), RecordingRuntimeError> {
    let temp = tempfile::tempdir().map_err(|_| RecordingRuntimeError::TransferState)?;
    let telemetry = SharedLifecycleTelemetry::default();
    let mut service = RecordingLifecycleService::new_with_test_inputs_and_telemetry(
        recording_transfer_store(temp.path())?,
        FailingPauseRuntime,
        telemetry.clone(),
        sufficient_disk_space_probe,
        TestRecordingMonotonicClock::fixed(fixed_monotonic_ns),
    );
    service.start_recording(
        &recording_start_command(VideoMode::Screen),
        RecordingStartPolicy::default(),
    )?;

    assert_eq!(
        service.control_recording(&recording_control_command(
            crate::protocol::RecordingControl::Pause,
            COMMAND_1_RECORDING_ID,
            4_500,
        )),
        Err(RecordingRuntimeError::RuntimeFailed)
    );

    let stop = service.control_recording(&recording_control_command(
        crate::protocol::RecordingControl::Stop,
        COMMAND_1_RECORDING_ID,
        6_000,
    ))?;
    let RecordingCommandEvent::Stopped(stopped) = &stop[2] else {
        return Err(RecordingRuntimeError::InvalidState);
    };
    assert_eq!(stopped.pending.duration_ms, 2_000);
    assert_eq!(
        telemetry.events(),
        vec![
            TelemetryLifecycleEvent::Start(COMMAND_1_RECORDING_ID.to_owned(), 987_654_321),
            TelemetryLifecycleEvent::Finish(COMMAND_1_RECORDING_ID.to_owned(), 987_654_321),
            TelemetryLifecycleEvent::Snapshot(COMMAND_1_RECORDING_ID.to_owned(), 2_000),
        ]
    );
    Ok(())
}

#[test]
fn recording_progress_ticks_are_rate_limited_and_state_changes_emit()
-> Result<(), RecordingRuntimeError> {
    let temp = tempfile::tempdir().map_err(|_| RecordingRuntimeError::TransferState)?;
    let mut service = recording_lifecycle_service(temp.path(), FakeRecordingRuntime)?;
    service.start_recording(
        &recording_start_command(VideoMode::Screen),
        RecordingStartPolicy::default(),
    )?;

    assert!(
        service
            .progress_tick("lease:connection-1", 4_100)?
            .is_empty()
    );
    assert!(
        service
            .progress_tick("lease:connection-1", 4_499)?
            .is_empty()
    );
    let tick = service.progress_tick("lease:connection-1", 4_500)?;
    let Some(RecordingCommandEvent::Progress(progress)) = tick.first() else {
        return Err(RecordingRuntimeError::InvalidState);
    };
    assert_eq!(progress.duration_ms, 500);
    assert_eq!(progress.status, RecordingProgressStatus::Recording);

    assert!(
        service
            .progress_tick("lease:connection-1", 4_800)?
            .is_empty()
    );
    let pause = service.control_recording(&recording_control_command(
        crate::protocol::RecordingControl::Pause,
        COMMAND_1_RECORDING_ID,
        4_850,
    ))?;
    let RecordingCommandEvent::Progress(progress) = &pause[0] else {
        return Err(RecordingRuntimeError::InvalidState);
    };
    assert_eq!(progress.duration_ms, 850);
    assert_eq!(progress.status, RecordingProgressStatus::Paused);
    assert!(
        service
            .progress_tick("lease:connection-1", 4_900)?
            .is_empty()
    );

    let resume = service.control_recording(&recording_control_command(
        crate::protocol::RecordingControl::Resume,
        COMMAND_1_RECORDING_ID,
        4_950,
    ))?;
    let RecordingCommandEvent::Progress(progress) = &resume[0] else {
        return Err(RecordingRuntimeError::InvalidState);
    };
    assert_eq!(progress.duration_ms, 850);
    assert_eq!(progress.status, RecordingProgressStatus::Recording);
    assert!(
        service
            .progress_tick("lease:connection-1", 5_200)?
            .is_empty()
    );
    let tick = service.progress_tick("lease:connection-1", 5_450)?;
    let Some(RecordingCommandEvent::Progress(progress)) = tick.first() else {
        return Err(RecordingRuntimeError::InvalidState);
    };
    assert_eq!(progress.duration_ms, 1_350);
    assert_eq!(progress.status, RecordingProgressStatus::Recording);
    Ok(())
}

#[test]
fn recording_progress_tick_finalizes_at_start_policy_duration_cap()
-> Result<(), RecordingRuntimeError> {
    let temp = tempfile::tempdir().map_err(|_| RecordingRuntimeError::TransferState)?;
    let telemetry = SharedLifecycleTelemetry::default();
    let mut service = RecordingLifecycleService::new_with_test_inputs_and_telemetry(
        recording_transfer_store(temp.path())?,
        FakeRecordingRuntime,
        telemetry.clone(),
        sufficient_disk_space_probe,
        TestRecordingMonotonicClock::fixed(fixed_monotonic_ns),
    );
    service.start_recording(
        &recording_start_command(VideoMode::Screen),
        RecordingStartPolicy {
            max_duration_ms: 2_000,
            ..RecordingStartPolicy::default()
        },
    )?;

    let messages = service.progress_tick("lease:connection-1", 6_000)?;

    assert_eq!(messages.len(), 3);
    let RecordingCommandEvent::Progress(stopping) = &messages[0] else {
        return Err(RecordingRuntimeError::InvalidState);
    };
    assert_eq!(stopping.status, RecordingProgressStatus::Stopping);
    assert_eq!(stopping.duration_ms, 2_000);
    let RecordingCommandEvent::Progress(finalizing) = &messages[1] else {
        return Err(RecordingRuntimeError::InvalidState);
    };
    assert_eq!(finalizing.status, RecordingProgressStatus::Finalizing);
    assert_eq!(finalizing.duration_ms, 2_000);
    let RecordingCommandEvent::Stopped(stopped) = &messages[2] else {
        return Err(RecordingRuntimeError::InvalidState);
    };
    assert_eq!(stopped.pending.duration_ms, 2_000);
    assert_eq!(stopped.pending.total_bytes, 11);
    assert_eq!(
        telemetry.events(),
        vec![
            TelemetryLifecycleEvent::Start(COMMAND_1_RECORDING_ID.to_owned(), 987_654_321),
            TelemetryLifecycleEvent::Finish(COMMAND_1_RECORDING_ID.to_owned(), 987_654_321),
            TelemetryLifecycleEvent::Snapshot(COMMAND_1_RECORDING_ID.to_owned(), 2_000),
        ]
    );
    assert!(
        service
            .progress_tick("lease:connection-1", 6_500)?
            .is_empty()
    );
    Ok(())
}

#[test]
fn recording_lifecycle_service_rejects_unsupported_mode_and_wrong_recording_id()
-> Result<(), RecordingRuntimeError> {
    let temp = tempfile::tempdir().map_err(|_| RecordingRuntimeError::TransferState)?;
    let mut service = recording_lifecycle_service(temp.path(), FakeRecordingRuntime)?;

    assert_eq!(
        service.start_recording(
            &recording_region_start_command(None),
            RecordingStartPolicy::default()
        ),
        Err(RecordingRuntimeError::UnsupportedMode)
    );

    assert!(
        service
            .start_recording(
                &recording_start_command(VideoMode::Screen),
                RecordingStartPolicy::default()
            )
            .is_ok()
    );
    assert_eq!(
        service.control_recording(&recording_control_command(
            crate::protocol::RecordingControl::Pause,
            "recording-other",
            4_500,
        )),
        Err(RecordingRuntimeError::InvalidState)
    );
    Ok(())
}

#[test]
fn recording_lifecycle_service_accepts_region_source_with_selection()
-> Result<(), RecordingRuntimeError> {
    let temp = tempfile::tempdir().map_err(|_| RecordingRuntimeError::TransferState)?;
    let mut service = recording_lifecycle_service(temp.path(), FakeRecordingRuntime)?;

    let messages = service.start_recording(
        &recording_region_start_command(Some(NativeRecordingRegion {
            x: 10,
            y: 20,
            width: 30,
            height: 40,
        })),
        RecordingStartPolicy::default(),
    )?;

    let RecordingCommandEvent::Started(started) = &messages[0] else {
        return Err(RecordingRuntimeError::InvalidState);
    };
    assert_eq!(started.source.mode, VideoMode::Region);
    assert_eq!(
        started.source.region.as_ref().map(|region| region.width),
        Some(30)
    );
    Ok(())
}

#[test]
fn recording_lifecycle_service_finalizes_stopped_recording_for_transfer()
-> Result<(), RecordingRuntimeError> {
    let temp = tempfile::tempdir().map_err(|_| RecordingRuntimeError::TransferState)?;
    let store = recording_transfer_store(temp.path())?;
    let mut service = RecordingLifecycleService::new_with_test_inputs(
        store.clone(),
        FakeRecordingRuntime,
        sufficient_disk_space_probe,
        TestRecordingMonotonicClock::fixed(fixed_monotonic_ns),
    );
    service.start_recording(
        &recording_start_command(VideoMode::Screen),
        RecordingStartPolicy::default(),
    )?;

    let messages = service.control_recording(&recording_control_command(
        crate::protocol::RecordingControl::Stop,
        COMMAND_1_RECORDING_ID,
        6_000,
    ))?;

    assert!(matches!(messages[2], RecordingCommandEvent::Stopped(_)));
    let chunk = store
        .recording_chunk_at(COMMAND_1_RECORDING_ID, 0, 7_000)
        .map_err(|_| RecordingRuntimeError::TransferState)?
        .ok_or(RecordingRuntimeError::TransferState)?;
    assert_eq!(chunk.base64, "aGVsbG8gd29ybGQ=");
    Ok(())
}

#[test]
fn recording_lifecycle_service_includes_sanitized_telemetry_snapshot()
-> Result<(), RecordingRuntimeError> {
    let temp = tempfile::tempdir().map_err(|_| RecordingRuntimeError::TransferState)?;
    let mut service = RecordingLifecycleService::new_with_test_inputs_and_telemetry(
        recording_transfer_store(temp.path())?,
        FakeRecordingRuntime,
        FixedTelemetrySource,
        sufficient_disk_space_probe,
        TestRecordingMonotonicClock::fixed(fixed_monotonic_ns),
    );
    service.start_recording(
        &recording_start_command(VideoMode::Screen),
        RecordingStartPolicy::default(),
    )?;

    let messages = service.control_recording(&recording_control_command(
        crate::protocol::RecordingControl::Stop,
        COMMAND_1_RECORDING_ID,
        6_000,
    ))?;

    let RecordingCommandEvent::Stopped(stopped) = &messages[2] else {
        return Err(RecordingRuntimeError::InvalidState);
    };
    let telemetry = stopped
        .telemetry
        .as_ref()
        .ok_or(RecordingRuntimeError::InvalidState)?;
    assert_eq!(telemetry.action_events[0]["label"], "Ctrl+S");
    assert_eq!(telemetry.signals.len(), 1);
    assert!(
        !serde_json::to_string(telemetry)
            .map_err(|_| RecordingRuntimeError::InvalidState)?
            .contains("typed text")
    );
    Ok(())
}

#[test]
fn recording_lifecycle_service_notifies_telemetry_lifecycle() -> Result<(), RecordingRuntimeError> {
    let temp = tempfile::tempdir().map_err(|_| RecordingRuntimeError::TransferState)?;
    let telemetry = SharedLifecycleTelemetry::default();
    let mut service = RecordingLifecycleService::new_with_test_inputs_and_telemetry(
        recording_transfer_store(temp.path())?,
        FakeRecordingRuntime,
        telemetry.clone(),
        sufficient_disk_space_probe,
        TestRecordingMonotonicClock::fixed(fixed_monotonic_ns),
    );
    service.start_recording(
        &recording_start_command(VideoMode::Screen),
        RecordingStartPolicy::default(),
    )?;
    service.control_recording(&recording_control_command(
        crate::protocol::RecordingControl::Pause,
        COMMAND_1_RECORDING_ID,
        4_500,
    ))?;
    service.control_recording(&recording_control_command(
        crate::protocol::RecordingControl::Resume,
        COMMAND_1_RECORDING_ID,
        5_000,
    ))?;
    service.control_recording(&recording_control_command(
        crate::protocol::RecordingControl::Stop,
        COMMAND_1_RECORDING_ID,
        6_000,
    ))?;

    assert_eq!(
        telemetry.events(),
        vec![
            TelemetryLifecycleEvent::Start(COMMAND_1_RECORDING_ID.to_owned(), 987_654_321),
            TelemetryLifecycleEvent::Pause(COMMAND_1_RECORDING_ID.to_owned(), 987_654_321),
            TelemetryLifecycleEvent::Resume(COMMAND_1_RECORDING_ID.to_owned(), 987_654_321),
            TelemetryLifecycleEvent::Finish(COMMAND_1_RECORDING_ID.to_owned(), 987_654_321),
            TelemetryLifecycleEvent::Snapshot(COMMAND_1_RECORDING_ID.to_owned(), 1_500),
        ]
    );
    Ok(())
}

#[test]
fn recording_lifecycle_service_notifies_telemetry_cancel_without_snapshot()
-> Result<(), RecordingRuntimeError> {
    let temp = tempfile::tempdir().map_err(|_| RecordingRuntimeError::TransferState)?;
    let telemetry = SharedLifecycleTelemetry::default();
    let mut service = RecordingLifecycleService::new_with_test_inputs_and_telemetry(
        recording_transfer_store(temp.path())?,
        FakeRecordingRuntime,
        telemetry.clone(),
        sufficient_disk_space_probe,
        TestRecordingMonotonicClock::fixed(fixed_monotonic_ns),
    );
    service.start_recording(
        &recording_start_command(VideoMode::Screen),
        RecordingStartPolicy::default(),
    )?;
    service.control_recording(&recording_control_command(
        crate::protocol::RecordingControl::Cancel,
        COMMAND_1_RECORDING_ID,
        4_500,
    ))?;

    assert_eq!(
        telemetry.events(),
        vec![
            TelemetryLifecycleEvent::Start(COMMAND_1_RECORDING_ID.to_owned(), 987_654_321),
            TelemetryLifecycleEvent::Cancel(COMMAND_1_RECORDING_ID.to_owned(), 987_654_321),
        ]
    );
    Ok(())
}

#[test]
fn recording_runtime_plan_uses_source_audio_and_encoder_facts()
-> Result<(), RecordingRuntimePlanError> {
    let command = recording_start_command(VideoMode::Screen);
    let plan = plan_recording_runtime(
        &command,
        RecordingSourceGeometry {
            width: 1920,
            height: 1080,
        },
        &recording_audio_endpoints(),
        recording_audio_facts(),
        encoder_capabilities(),
        recording_quality_limits(),
    )?;

    assert_eq!(
        plan.audio_selection
            .microphone
            .as_ref()
            .map(|mic| mic.stable_id.as_str()),
        Some("mic-default")
    );
    assert!(plan.mix.microphone.is_some());
    assert!(plan.mix.system_loopback.is_some());
    assert_eq!(plan.encoder.width, 1920);
    assert_eq!(plan.encoder.height, 1080);
    assert_eq!(plan.encoder.fps, 30);
    assert_eq!(plan.effective_quality.effective_fps.as_u64(), Some(30));
    Ok(())
}

#[test]
fn recording_runtime_plan_rejects_unavailable_requested_microphone() {
    let mut command = recording_start_command(VideoMode::Screen);
    command.source.microphone_device_id = Some("missing-mic".to_owned());

    assert_eq!(
        plan_recording_runtime(
            &command,
            RecordingSourceGeometry {
                width: 1920,
                height: 1080,
            },
            &recording_audio_endpoints(),
            recording_audio_facts(),
            encoder_capabilities(),
            recording_quality_limits(),
        ),
        Err(RecordingRuntimePlanError::Audio(
            AudioPlanError::MissingMicrophone
        ))
    );
}

#[test]
fn recording_runtime_plan_rejects_excessive_audio_drift() {
    let mut facts = recording_audio_facts();
    facts.system_loopback_clock_drift_ppm = 750;

    assert_eq!(
        plan_recording_runtime(
            &recording_start_command(VideoMode::Screen),
            RecordingSourceGeometry {
                width: 1920,
                height: 1080,
            },
            &recording_audio_endpoints(),
            facts,
            encoder_capabilities(),
            recording_quality_limits(),
        ),
        Err(RecordingRuntimePlanError::Mix(
            MixPlanError::ExcessiveClockDrift
        ))
    );
}

#[test]
fn recording_runtime_plan_rejects_encoder_capability_mismatch() {
    let mut capabilities = encoder_capabilities();
    capabilities.max_width = 1280;

    assert_eq!(
        plan_recording_runtime(
            &recording_start_command(VideoMode::Screen),
            RecordingSourceGeometry {
                width: 1920,
                height: 1080,
            },
            &recording_audio_endpoints(),
            recording_audio_facts(),
            capabilities,
            recording_quality_limits(),
        ),
        Err(RecordingRuntimePlanError::Encoder(
            EncoderPlanError::InvalidGeometry
        ))
    );
}

#[test]
fn recording_preflight_returns_plan_for_supported_facts() -> Result<(), RecordingRuntimeError> {
    let plan = preflight_recording_start(
        &recording_start_command(VideoMode::Screen),
        &RecordingPreflightFacts {
            source: RecordingSourceGeometry {
                width: 1920,
                height: 1080,
            },
            audio_endpoints: recording_audio_endpoints(),
            audio_facts: recording_audio_facts(),
            encoder_capabilities: encoder_capabilities(),
            quality_limits: recording_quality_limits(),
        },
    )?;

    assert_eq!(plan.source.width, 1920);
    assert_eq!(plan.encoder.fps, 30);
    assert!(plan.mix.microphone.is_some());
    assert!(plan.mix.system_loopback.is_some());
    Ok(())
}

#[test]
fn recording_preflight_maps_missing_audio_to_unsupported_mode() {
    let mut command = recording_start_command(VideoMode::Screen);
    command.requested_quality.audio_source_mode = NativeAudioSourceMode::System;

    assert_eq!(
        preflight_recording_start(
            &command,
            &RecordingPreflightFacts {
                source: RecordingSourceGeometry {
                    width: 1920,
                    height: 1080,
                },
                audio_endpoints: vec![AudioEndpoint::microphone(
                    "mic-default",
                    "Default Microphone",
                    true,
                    AudioEndpointState::Available,
                )],
                audio_facts: recording_audio_facts(),
                encoder_capabilities: encoder_capabilities(),
                quality_limits: recording_quality_limits(),
            },
        ),
        Err(RecordingRuntimeError::UnsupportedMode)
    );
}

#[test]
fn recording_lifecycle_service_discards_partial_media_when_runtime_stop_fails()
-> Result<(), RecordingRuntimeError> {
    let temp = tempfile::tempdir().map_err(|_| RecordingRuntimeError::TransferState)?;
    let store = recording_transfer_store(temp.path())?;
    let telemetry = SharedLifecycleTelemetry::default();
    let mut service = RecordingLifecycleService::new_with_test_inputs_and_telemetry(
        store,
        FailingStopRuntime,
        telemetry.clone(),
        sufficient_disk_space_probe,
        TestRecordingMonotonicClock::fixed(fixed_monotonic_ns),
    );
    service.start_recording(
        &recording_start_command(VideoMode::Screen),
        RecordingStartPolicy::default(),
    )?;

    assert_eq!(
        service.control_recording(&recording_control_command(
            crate::protocol::RecordingControl::Stop,
            COMMAND_1_RECORDING_ID,
            6_000,
        )),
        Err(RecordingRuntimeError::RuntimeFailed)
    );
    assert!(!temp.path().join(COMMAND_1_RECORDING_JSON).exists());
    assert!(!temp.path().join(COMMAND_1_RECORDING_MP4).exists());
    assert_eq!(
        telemetry.events(),
        vec![
            TelemetryLifecycleEvent::Start(COMMAND_1_RECORDING_ID.to_owned(), 987_654_321),
            TelemetryLifecycleEvent::Finish(COMMAND_1_RECORDING_ID.to_owned(), 987_654_321),
            TelemetryLifecycleEvent::Discard(COMMAND_1_RECORDING_ID.to_owned()),
        ]
    );

    let messages = service.start_recording(
        &recording_start_command(VideoMode::Screen),
        RecordingStartPolicy::default(),
    )?;
    assert!(matches!(messages[0], RecordingCommandEvent::Started(_)));
    Ok(())
}

#[test]
fn recording_lifecycle_service_discards_temp_record_when_runtime_start_fails()
-> Result<(), RecordingRuntimeError> {
    let temp = tempfile::tempdir().map_err(|_| RecordingRuntimeError::TransferState)?;
    let mut service = RecordingLifecycleService::new_with_test_inputs(
        recording_transfer_store(temp.path())?,
        FailingStartRuntime,
        sufficient_disk_space_probe,
        TestRecordingMonotonicClock::fixed(fixed_monotonic_ns),
    );

    assert_eq!(
        service.start_recording(
            &recording_start_command(VideoMode::Screen),
            RecordingStartPolicy::default()
        ),
        Err(RecordingRuntimeError::RuntimeFailed)
    );
    assert!(!temp.path().join(COMMAND_1_RECORDING_JSON).exists());
    assert!(!temp.path().join(COMMAND_1_RECORDING_MP4).exists());
    Ok(())
}

#[test]
fn recording_lifecycle_service_resets_after_transfer_finalization_failure()
-> Result<(), RecordingRuntimeError> {
    let temp = tempfile::tempdir().map_err(|_| RecordingRuntimeError::TransferState)?;
    let mut service = RecordingLifecycleService::new_with_test_inputs(
        recording_transfer_store(temp.path())?,
        InvalidArtifactRuntime,
        sufficient_disk_space_probe,
        TestRecordingMonotonicClock::fixed(fixed_monotonic_ns),
    );
    service.start_recording(
        &recording_start_command(VideoMode::Screen),
        RecordingStartPolicy::default(),
    )?;

    assert_eq!(
        service.control_recording(&recording_control_command(
            crate::protocol::RecordingControl::Stop,
            COMMAND_1_RECORDING_ID,
            6_000,
        )),
        Err(RecordingRuntimeError::TransferState)
    );
    assert!(!temp.path().join(COMMAND_1_RECORDING_JSON).exists());
    assert!(!temp.path().join(COMMAND_1_RECORDING_MP4).exists());

    let messages = service.start_recording(
        &recording_start_command(VideoMode::Screen),
        RecordingStartPolicy::default(),
    )?;
    assert!(matches!(messages[0], RecordingCommandEvent::Started(_)));
    Ok(())
}

#[test]
fn unsupported_recording_runtime_fails_closed() -> Result<(), RecordingRuntimeError> {
    let temp = tempfile::tempdir().map_err(|_| RecordingRuntimeError::TransferState)?;
    let mut runtime = UnsupportedRecordingRuntime;
    let command = recording_start_command(VideoMode::Screen);
    let media_path = temp.path().join(COMMAND_1_RECORDING_MP4);

    assert_eq!(
        runtime.start_recording(RecordingRuntimeStartRequest {
            request: &command,
            recording_id: COMMAND_1_RECORDING_ID,
            media_path: &media_path,
        }),
        Err(RecordingRuntimeError::RuntimeUnavailable)
    );
    Ok(())
}

#[test]
fn recording_disk_guard_uses_capped_percentage_reserve() -> Result<(), RecordingDiskGuardError> {
    assert_eq!(
        recording_disk_reserve_bytes(1_024 * 1_024 * 1_024)?,
        512 * 1024 * 1024
    );
    assert_eq!(
        recording_disk_reserve_bytes(20 * 1024 * 1024 * 1024)?,
        2 * 1024 * 1024 * 1024
    );
    assert_eq!(
        recording_disk_reserve_bytes(200 * 1024 * 1024 * 1024)?,
        2 * 1024 * 1024 * 1024
    );
    assert_eq!(
        recording_disk_reserve_bytes(0),
        Err(RecordingDiskGuardError::InvalidCapacity)
    );
    assert_eq!(
        ensure_recording_disk_headroom(
            RecordingDiskSpace {
                available_bytes: 10 * 1024 * 1024 * 1024,
                capacity_bytes: 20 * 1024 * 1024 * 1024,
            },
            8 * 1024 * 1024 * 1024,
        ),
        Ok(())
    );
    Ok(())
}

#[test]
fn recording_disk_guard_rejects_pressure_before_temp_record() -> Result<(), RecordingRuntimeError> {
    let temp = tempfile::tempdir().map_err(|_| RecordingRuntimeError::TransferState)?;
    let store = recording_transfer_store(temp.path())?;
    let mut service = RecordingLifecycleService::new_with_test_inputs(
        store.clone(),
        FakeRecordingRuntime,
        pressure_disk_space_probe,
        TestRecordingMonotonicClock::fixed(fixed_monotonic_ns),
    );

    assert_eq!(
        service.start_recording(
            &recording_start_command(VideoMode::Screen),
            RecordingStartPolicy::default()
        ),
        Err(RecordingRuntimeError::DiskPressure)
    );
    assert!(!temp.path().join(COMMAND_1_RECORDING_JSON).exists());
    assert!(!temp.path().join(COMMAND_1_RECORDING_MP4).exists());
    Ok(())
}
