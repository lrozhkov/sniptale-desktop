use std::sync::mpsc;

use crate::tray::{
    ExtensionIntentActionId, TrayActionKind, TrayActionRegistry, TrayCommandSource, TrayMenuItem,
    TrayNoticeKind, TrayRecordingStatus, TrayRuntimeError, TrayRuntimeEvent,
};

use super::{
    TrayWorker, TrayWorkerControl, TrayWorkerRegistrySink, runner::reconcile_queued_commands,
    view::command_action_ids,
};
use crate::app::{
    resident::{
        AgentEventHub, AgentOutboundEvent, AgentShutdownSignal, AgentStartupStatus, WorkerCommand,
    },
    responses::{NativeHostSideEffectError, NativeHostSideEffects},
};

#[test]
fn tray_worker_projects_registry_into_menu_commands() {
    let mut worker = tray_worker();
    let events = AgentEventHub::default();
    let mut source = FakeTrayCommandSource::default();
    let (shutdown_sender, _shutdown_receiver) = shutdown_channel();

    let result = worker.handle_command(
        apply_registry_command(registry()),
        &events,
        &shutdown_sender,
        &mut source,
    );

    assert_eq!(result, TrayWorkerControl::Continue);
    assert_eq!(source.last_registry_result(), Some(Ok(())));
    assert_eq!(
        source.tooltips.last().map(String::as_str),
        Some("Sniptale: connected, idle, unsupported cache schema")
    );
    assert_eq!(
        command_action_ids(&worker.commands),
        vec![
            "screenshot-screen",
            "mode-manual",
            "open-extension-settings",
            "open-settings",
            "quit",
        ]
    );
}

#[test]
fn tray_worker_renders_fallback_menu_before_settings_sync() -> Result<(), TrayRuntimeError> {
    let mut worker = tray_worker();
    let mut source = FakeTrayCommandSource::default();

    worker.render_fallback_menu(&mut source)?;

    assert_eq!(source.render_count, 1);
    assert_eq!(
        source.tooltips,
        vec!["Sniptale: disconnected, idle, unsupported cache schema"]
    );
    assert_eq!(
        command_action_ids(&worker.commands),
        vec!["mode-manual", "open-settings", "quit"]
    );
    Ok(())
}

#[test]
fn tray_worker_renders_recording_phase_updates() {
    let mut worker = tray_worker();
    let events = AgentEventHub::default();
    let mut source = FakeTrayCommandSource::default();
    let (shutdown_sender, _shutdown_receiver) = shutdown_channel();

    worker.handle_command(
        WorkerCommand::UpdateTrayRecordingStatus {
            status: TrayRecordingStatus::Finalizing,
        },
        &events,
        &shutdown_sender,
        &mut source,
    );

    assert_eq!(
        source.tooltips.last().map(String::as_str),
        Some("Sniptale: disconnected, finalizing, unsupported cache schema")
    );
}

#[test]
fn tray_worker_disables_extension_actions_after_connection_closes() {
    let mut worker = tray_worker();
    let events = AgentEventHub::default();
    let mut source = FakeTrayCommandSource::default();
    let (shutdown_sender, _shutdown_receiver) = shutdown_channel();

    worker.handle_command(
        apply_registry_command(registry()),
        &events,
        &shutdown_sender,
        &mut source,
    );
    worker.connection_state.set_connected(1);
    assert!(worker.connection_state.disconnect_if_current(1));
    worker.handle_command(
        WorkerCommand::RefreshTrayConnectionStatus,
        &events,
        &shutdown_sender,
        &mut source,
    );

    assert_eq!(
        source.tooltips.last().map(String::as_str),
        Some("Sniptale: disconnected, idle, unsupported cache schema")
    );
    assert!(
        command_action_ids(&worker.commands)
            .iter()
            .all(|action_id| *action_id != "screenshot-screen")
    );
}

#[test]
fn tray_worker_drops_stale_media_commands_after_recording_starts() -> Result<(), String> {
    let mut worker = tray_worker();
    let events = AgentEventHub::default();
    let receiver = events.subscribe();
    let mut source = FakeTrayCommandSource::default();
    let (shutdown_sender, _shutdown_receiver) = shutdown_channel();
    worker.handle_command(
        apply_registry_command(registry()),
        &events,
        &shutdown_sender,
        &mut source,
    );
    let stale_capture = command_id_for(&worker, "screenshot-screen")?;

    worker.handle_command(
        WorkerCommand::UpdateTrayRecordingStatus {
            status: TrayRecordingStatus::Recording,
        },
        &events,
        &shutdown_sender,
        &mut source,
    );
    assert!(
        command_action_ids(&worker.commands)
            .iter()
            .all(|action_id| *action_id != "screenshot-screen")
    );

    worker.handle_command(
        WorkerCommand::TrayMenuCommandSelected {
            command_id: stale_capture,
        },
        &events,
        &shutdown_sender,
        &mut source,
    );
    assert!(receiver.try_recv().is_err());
    Ok(())
}

#[test]
fn tray_worker_reconciles_recording_update_queued_while_menu_was_open() -> Result<(), String> {
    let mut worker = tray_worker();
    let events = AgentEventHub::default();
    let receiver = events.subscribe();
    let mut source = FakeTrayCommandSource::default();
    let (shutdown_sender, _shutdown_receiver) = shutdown_channel();
    worker.handle_command(
        apply_registry_command(registry()),
        &events,
        &shutdown_sender,
        &mut source,
    );
    let stale_capture = command_id_for(&worker, "screenshot-screen")?;
    let source_events = vec![TrayRuntimeEvent::MenuCommand {
        command_id: stale_capture,
    }];
    let (command_sender, command_receiver) = mpsc::sync_channel(1);
    command_sender
        .send(WorkerCommand::UpdateTrayRecordingStatus {
            status: TrayRecordingStatus::Recording,
        })
        .map_err(|_| "recording update must queue".to_owned())?;

    assert_eq!(
        reconcile_queued_commands(
            &mut worker,
            &command_receiver,
            &events,
            &shutdown_sender,
            &mut source,
        ),
        TrayWorkerControl::Continue
    );
    worker.dispatch_source_events(source_events, &events, &shutdown_sender, &mut source);

    assert!(receiver.try_recv().is_err());
    Ok(())
}

#[test]
fn extension_tray_recording_stop_routes_to_recording_owner() -> Result<(), String> {
    let (recording_sender, recording_receiver) = mpsc::sync_channel(1);
    let mut worker = TrayWorker::new(
        AgentStartupStatus::Operational,
        crate::settings::ActiveSettings::extension_managed_default(),
        None,
        super::ManualWorkerSenders::new(None, Some(recording_sender), None, None, None),
    );
    let events = AgentEventHub::default();
    let mut source = FakeTrayCommandSource::default();
    let (shutdown_sender, _shutdown_receiver) = shutdown_channel();
    worker.handle_command(
        apply_registry_command(registry()),
        &events,
        &shutdown_sender,
        &mut source,
    );
    worker.handle_command(
        WorkerCommand::UpdateTrayRecordingStatus {
            status: TrayRecordingStatus::Recording,
        },
        &events,
        &shutdown_sender,
        &mut source,
    );
    let stop = command_id_for(&worker, "recording-stop")?;

    worker.handle_command(
        WorkerCommand::TrayMenuCommandSelected { command_id: stop },
        &events,
        &shutdown_sender,
        &mut source,
    );

    assert!(matches!(
        recording_receiver.try_recv(),
        Ok(WorkerCommand::ControlActiveRecording {
            control: crate::protocol::RecordingControl::Stop
        })
    ));
    Ok(())
}

#[test]
fn tray_worker_dispatches_menu_commands_to_outbound_events() -> Result<(), String> {
    let mut worker = tray_worker();
    let events = AgentEventHub::default();
    let receiver = events.subscribe();
    let mut source = FakeTrayCommandSource::default();
    let (shutdown_sender, _shutdown_receiver) = shutdown_channel();
    worker.handle_command(
        apply_registry_command(registry()),
        &events,
        &shutdown_sender,
        &mut source,
    );

    let open_settings = command_id_for(&worker, "open-settings")?;
    worker.handle_command(
        WorkerCommand::TrayMenuCommandSelected {
            command_id: open_settings,
        },
        &events,
        &shutdown_sender,
        &mut source,
    );
    assert!(receiver.try_recv().is_err());
    assert_eq!(
        source.settings_views,
        vec![crate::tray::settings_window::model::SettingsWindowView::ExtensionManagedReadOnly]
    );
    assert_eq!(source.notices.len(), 0);

    let open_extension_settings = command_id_for(&worker, "open-extension-settings")?;
    worker.handle_command(
        WorkerCommand::TrayMenuCommandSelected {
            command_id: open_extension_settings,
        },
        &events,
        &shutdown_sender,
        &mut source,
    );
    assert!(matches!(
        receiver.try_recv(),
        Ok(AgentOutboundEvent::OpenSettingsRequested { .. })
    ));

    let capture = command_id_for(&worker, "screenshot-screen")?;
    worker.handle_command(
        WorkerCommand::TrayMenuCommandSelected {
            command_id: capture,
        },
        &events,
        &shutdown_sender,
        &mut source,
    );
    assert!(matches!(
        receiver.try_recv(),
        Ok(AgentOutboundEvent::TrayActionRequested { action_id, .. })
        if action_id == "screenshot-screen"
    ));
    Ok(())
}

#[test]
fn extension_mode_picker_opens_setup_without_changing_authority() -> Result<(), String> {
    let mut worker = tray_worker();
    let events = AgentEventHub::default();
    let mut source = FakeTrayCommandSource {
        settings_destination: Some(std::path::PathBuf::from(r"C:\Capture")),
        ..FakeTrayCommandSource::default()
    };
    let (shutdown_sender, _shutdown_receiver) = shutdown_channel();

    worker
        .render_fallback_menu(&mut source)
        .map_err(|error| error.to_string())?;
    let manual_mode = command_id_for(&worker, "mode-manual")?;
    worker.handle_command(
        WorkerCommand::TrayMenuCommandSelected {
            command_id: manual_mode,
        },
        &events,
        &shutdown_sender,
        &mut source,
    );

    assert_eq!(
        source.settings_views,
        vec![crate::tray::settings_window::model::SettingsWindowView::ManualDraft]
    );
    assert_eq!(
        worker
            .active_settings
            .epoch()
            .map_err(|error| error.to_string())?
            .stable_mode(),
        Some(crate::settings::OperatingMode::ExtensionManaged)
    );
    Ok(())
}

#[test]
fn manual_mode_picker_cancel_leaves_authority_and_window_untouched() -> Result<(), String> {
    let mut worker = tray_worker();
    let events = AgentEventHub::default();
    let mut source = FakeTrayCommandSource::default();
    let (shutdown_sender, _shutdown_receiver) = shutdown_channel();
    worker
        .render_fallback_menu(&mut source)
        .map_err(|error| error.to_string())?;

    let command_id = command_id_for(&worker, "mode-manual")?;
    worker.handle_command(
        WorkerCommand::TrayMenuCommandSelected { command_id },
        &events,
        &shutdown_sender,
        &mut source,
    );

    assert_eq!(source.settings_views.len(), 0);
    assert_eq!(source.notices.len(), 0);
    assert_eq!(
        worker
            .active_settings
            .epoch()
            .map_err(|error| format!("{error:?}"))?
            .stable_mode(),
        Some(crate::settings::OperatingMode::ExtensionManaged)
    );
    Ok(())
}

#[test]
fn manual_mode_picker_error_rejects_without_changing_authority() -> Result<(), String> {
    let mut worker = tray_worker();
    let events = AgentEventHub::default();
    let mut source = FakeTrayCommandSource {
        fail_settings_destination: true,
        ..FakeTrayCommandSource::default()
    };
    let (shutdown_sender, _shutdown_receiver) = shutdown_channel();
    worker
        .render_fallback_menu(&mut source)
        .map_err(|error| error.to_string())?;

    let command_id = command_id_for(&worker, "mode-manual")?;
    worker.handle_command(
        WorkerCommand::TrayMenuCommandSelected { command_id },
        &events,
        &shutdown_sender,
        &mut source,
    );

    assert_eq!(source.settings_views.len(), 0);
    assert_eq!(source.notices, vec![TrayNoticeKind::ActionRejected]);
    assert_eq!(
        worker
            .active_settings
            .epoch()
            .map_err(|error| format!("{error:?}"))?
            .stable_mode(),
        Some(crate::settings::OperatingMode::ExtensionManaged)
    );
    Ok(())
}

#[test]
fn tray_worker_open_settings_notifies_when_extension_session_is_absent() -> Result<(), String> {
    let mut worker = tray_worker();
    let events = AgentEventHub::default();
    let mut source = FakeTrayCommandSource::default();
    let (shutdown_sender, _shutdown_receiver) = shutdown_channel();
    worker.handle_command(
        apply_registry_command(registry()),
        &events,
        &shutdown_sender,
        &mut source,
    );

    let open_settings = command_id_for(&worker, "open-settings")?;
    worker.handle_command(
        WorkerCommand::TrayMenuCommandSelected {
            command_id: open_settings,
        },
        &events,
        &shutdown_sender,
        &mut source,
    );

    assert_eq!(source.notices.len(), 0);
    assert_eq!(
        source.settings_views,
        vec![crate::tray::settings_window::model::SettingsWindowView::ExtensionManagedReadOnly]
    );
    Ok(())
}

#[test]
fn tray_worker_configured_action_notifies_when_extension_session_is_absent() -> Result<(), String> {
    let mut worker = tray_worker();
    let events = AgentEventHub::default();
    let mut source = FakeTrayCommandSource::default();
    let (shutdown_sender, _shutdown_receiver) = shutdown_channel();
    worker.handle_command(
        apply_registry_command(registry()),
        &events,
        &shutdown_sender,
        &mut source,
    );

    let capture = command_id_for(&worker, "screenshot-screen")?;
    worker.handle_command(
        WorkerCommand::TrayMenuCommandSelected {
            command_id: capture,
        },
        &events,
        &shutdown_sender,
        &mut source,
    );

    assert_eq!(source.notices, vec![TrayNoticeKind::ExtensionDisconnected]);
    Ok(())
}

#[test]
fn tray_worker_configured_action_notifies_when_outbound_queue_is_full() -> Result<(), String> {
    let mut worker = tray_worker();
    let events = AgentEventHub::default();
    let _receiver = events.subscribe();
    let mut source = FakeTrayCommandSource::default();
    let (shutdown_sender, _shutdown_receiver) = shutdown_channel();
    worker.handle_command(
        apply_registry_command(registry()),
        &events,
        &shutdown_sender,
        &mut source,
    );
    for index in 0..16 {
        events.publish(&AgentOutboundEvent::TrayActionRequested {
            action_id: format!("capture-{index}"),
            invocation_id: format!("tray-{index}"),
            requested_at_epoch_ms: 4_000,
        });
    }

    let capture = command_id_for(&worker, "screenshot-screen")?;
    worker.handle_command(
        WorkerCommand::TrayMenuCommandSelected {
            command_id: capture,
        },
        &events,
        &shutdown_sender,
        &mut source,
    );

    assert_eq!(source.notices, vec![TrayNoticeKind::ActionRejected]);
    Ok(())
}

#[test]
fn tray_worker_drains_runtime_source_events() -> Result<(), String> {
    let mut worker = tray_worker();
    let events = AgentEventHub::default();
    let receiver = events.subscribe();
    let mut source = FakeTrayCommandSource::default();
    let (shutdown_sender, _shutdown_receiver) = shutdown_channel();
    worker.handle_command(
        apply_registry_command(registry()),
        &events,
        &shutdown_sender,
        &mut source,
    );
    source.events = vec![TrayRuntimeEvent::MenuCommand {
        command_id: command_id_for(&worker, "screenshot-screen")?,
    }];

    let source_events = TrayWorker::drain_source_events(&mut source);
    worker.dispatch_source_events(source_events, &events, &shutdown_sender, &mut source);

    assert!(matches!(
        receiver.try_recv(),
        Ok(AgentOutboundEvent::TrayActionRequested { action_id, .. })
            if action_id == "screenshot-screen"
    ));
    Ok(())
}

#[test]
fn tray_worker_reports_runtime_render_failure_without_committing_registry() {
    let mut worker = tray_worker();
    let events = AgentEventHub::default();
    let mut source = FakeTrayCommandSource::default();
    let (shutdown_sender, _shutdown_receiver) = shutdown_channel();

    worker.handle_command(
        apply_registry_command(registry()),
        &events,
        &shutdown_sender,
        &mut source,
    );
    source.fail_next_render = true;

    let result = worker.handle_command(
        apply_registry_command(TrayActionRegistry::new(Vec::new())),
        &events,
        &shutdown_sender,
        &mut source,
    );

    assert_eq!(result, TrayWorkerControl::Continue);
    assert_eq!(
        source.last_registry_result(),
        Some(Err(TrayRuntimeError::RenderFailed))
    );
    assert_eq!(
        command_action_ids(&worker.commands),
        vec![
            "screenshot-screen",
            "mode-manual",
            "open-extension-settings",
            "open-settings",
            "quit",
        ]
    );
}

#[test]
fn tray_worker_shows_requested_notice() {
    let mut worker = tray_worker();
    let events = AgentEventHub::default();
    let mut source = FakeTrayCommandSource::default();
    let (shutdown_sender, _shutdown_receiver) = shutdown_channel();

    let result = worker.handle_command(
        WorkerCommand::ShowTrayNotice {
            kind: TrayNoticeKind::ActionRejected,
        },
        &events,
        &shutdown_sender,
        &mut source,
    );

    assert_eq!(result, TrayWorkerControl::Continue);
    assert_eq!(source.notices, vec![TrayNoticeKind::ActionRejected]);
}

#[test]
fn tray_worker_quit_requests_agent_shutdown() -> Result<(), TrayRuntimeError> {
    let mut worker = tray_worker();
    let events = AgentEventHub::default();
    let mut source = FakeTrayCommandSource::default();
    let (shutdown_sender, shutdown_receiver) = shutdown_channel();

    worker.render_fallback_menu(&mut source)?;
    let quit = command_id_for(&worker, "quit").map_err(|_| TrayRuntimeError::RenderFailed)?;
    worker.handle_command(
        WorkerCommand::TrayMenuCommandSelected { command_id: quit },
        &events,
        &shutdown_sender,
        &mut source,
    );

    assert_eq!(shutdown_receiver.try_recv(), Ok(AgentShutdownSignal::Stop));
    Ok(())
}

#[test]
fn tray_worker_confirmed_quit_stops_recording_then_waits_for_idle() -> Result<(), String> {
    let (recording_sender, recording_receiver) = mpsc::sync_channel(1);
    let mut worker = TrayWorker::new(
        AgentStartupStatus::Operational,
        crate::settings::ActiveSettings::extension_managed_default(),
        None,
        super::ManualWorkerSenders::new(None, Some(recording_sender), None, None, None),
    );
    let events = AgentEventHub::default();
    let mut source = FakeTrayCommandSource {
        confirm_exit: true,
        ..FakeTrayCommandSource::default()
    };
    let (shutdown_sender, shutdown_receiver) = shutdown_channel();
    worker
        .render_fallback_menu(&mut source)
        .map_err(|error| error.to_string())?;
    worker.handle_command(
        WorkerCommand::UpdateTrayRecordingStatus {
            status: TrayRecordingStatus::Recording,
        },
        &events,
        &shutdown_sender,
        &mut source,
    );
    let quit = command_id_for(&worker, "quit")?;

    worker.handle_command(
        WorkerCommand::TrayMenuCommandSelected { command_id: quit },
        &events,
        &shutdown_sender,
        &mut source,
    );
    assert!(matches!(
        recording_receiver.try_recv(),
        Ok(WorkerCommand::ControlActiveRecording {
            control: crate::protocol::RecordingControl::Stop
        })
    ));
    assert!(shutdown_receiver.try_recv().is_err());

    worker.handle_command(
        WorkerCommand::UpdateTrayRecordingStatus {
            status: TrayRecordingStatus::Idle,
        },
        &events,
        &shutdown_sender,
        &mut source,
    );
    assert_eq!(shutdown_receiver.try_recv(), Ok(AgentShutdownSignal::Stop));
    assert_eq!(source.exit_confirmations, 1);
    Ok(())
}

#[test]
fn tray_worker_rejects_quit_during_finalization() -> Result<(), String> {
    let mut worker = tray_worker();
    let events = AgentEventHub::default();
    let mut source = FakeTrayCommandSource {
        confirm_exit: true,
        ..FakeTrayCommandSource::default()
    };
    let (shutdown_sender, shutdown_receiver) = shutdown_channel();
    worker
        .render_fallback_menu(&mut source)
        .map_err(|error| error.to_string())?;
    worker.handle_command(
        WorkerCommand::UpdateTrayRecordingStatus {
            status: TrayRecordingStatus::Finalizing,
        },
        &events,
        &shutdown_sender,
        &mut source,
    );
    let quit = command_id_for(&worker, "quit")?;

    worker.handle_command(
        WorkerCommand::TrayMenuCommandSelected { command_id: quit },
        &events,
        &shutdown_sender,
        &mut source,
    );

    assert!(shutdown_receiver.try_recv().is_err());
    assert_eq!(source.exit_confirmations, 0);
    assert_eq!(source.notices, vec![TrayNoticeKind::ActionRejected]);
    Ok(())
}

#[test]
fn manual_mode_picker_opens_a_staged_extension_return() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let profile_path = directory.path().join("local-profile.json");
    let profile = crate::settings::ensure_local_profile_from_path(Some(profile_path.clone()))
        .map_err(|_| std::io::Error::other("profile"))?;
    let settings = crate::settings::ActiveSettings::from_profile(&profile);
    let draft = crate::settings::LocalProfileDraft::manual(
        crate::settings::ManualProfileDraft::with_defaults(
            "destination".to_owned(),
            directory.path().to_string_lossy().into_owned(),
        ),
    );
    crate::settings::apply_local_profile_draft_from_path(Some(profile_path), &settings, &draft)
        .map_err(|_| std::io::Error::other("manual profile"))?;
    let dispatcher = crate::app::resident::control::manual_dispatch::ManualActionDispatcher::new(
        settings.clone(),
        None,
        None,
        None,
    );
    let settings_probe = settings.clone();
    let mut worker = TrayWorker::new(
        AgentStartupStatus::Operational,
        settings,
        Some(dispatcher),
        super::ManualWorkerSenders::default(),
    );
    let events = AgentEventHub::default();
    let mut source = FakeTrayCommandSource::default();
    let (shutdown_sender, _shutdown_receiver) = shutdown_channel();

    worker.render_fallback_menu(&mut source)?;
    let extension_mode =
        command_id_for(&worker, "mode-extension-managed").map_err(std::io::Error::other)?;
    worker.handle_command(
        WorkerCommand::TrayMenuCommandSelected {
            command_id: extension_mode,
        },
        &events,
        &shutdown_sender,
        &mut source,
    );

    assert_eq!(
        source.settings_views,
        vec![crate::tray::settings_window::model::SettingsWindowView::ExtensionManagedDraft]
    );
    assert_eq!(
        settings_probe.epoch()?.stable_mode(),
        Some(crate::settings::OperatingMode::Manual)
    );
    Ok(())
}

#[test]
fn tray_registry_sink_submits_worker_command() -> Result<(), NativeHostSideEffectError> {
    let (sender, receiver) = mpsc::sync_channel(1);
    let mut sink = TrayWorkerRegistrySink::new(Some(sender));

    let apply = std::thread::spawn(move || sink.apply_tray_registry(registry()));

    let command = receiver
        .recv()
        .map_err(|_| NativeHostSideEffectError::CommandDispatch)?;
    let WorkerCommand::ApplyTrayRegistry {
        registry,
        result_sender,
    } = command
    else {
        return Err(NativeHostSideEffectError::CommandDispatch);
    };
    assert_eq!(registry.actions().len(), 1);
    result_sender
        .send(Ok(()))
        .map_err(|_| NativeHostSideEffectError::CommandDispatch)?;
    apply
        .join()
        .map_err(|_| NativeHostSideEffectError::CommandDispatch)??;
    Ok(())
}

#[test]
fn tray_registry_sink_reports_worker_runtime_rejection() -> Result<(), NativeHostSideEffectError> {
    let (sender, receiver) = mpsc::sync_channel(1);
    let mut sink = TrayWorkerRegistrySink::new(Some(sender));

    let apply = std::thread::spawn(move || sink.apply_tray_registry(registry()));
    let command = receiver
        .recv()
        .map_err(|_| NativeHostSideEffectError::CommandDispatch)?;
    let WorkerCommand::ApplyTrayRegistry { result_sender, .. } = command else {
        return Err(NativeHostSideEffectError::CommandDispatch);
    };
    result_sender
        .send(Err(TrayRuntimeError::RenderFailed))
        .map_err(|_| NativeHostSideEffectError::CommandDispatch)?;

    assert_eq!(
        apply
            .join()
            .map_err(|_| NativeHostSideEffectError::CommandDispatch)?,
        Err(NativeHostSideEffectError::TrayApply)
    );
    Ok(())
}

#[test]
fn tray_registry_sink_reports_worker_backpressure() {
    let (sender, _receiver) = mpsc::sync_channel(0);
    let mut sink = TrayWorkerRegistrySink::new(Some(sender));

    assert_eq!(
        sink.apply_tray_registry(registry()),
        Err(NativeHostSideEffectError::TrayApply)
    );
}

#[test]
fn tray_registry_sink_marks_shared_state_offline_when_worker_queue_is_busy() {
    let (sender, _receiver) = mpsc::sync_channel(0);
    let state = super::TrayConnectionState::default();
    state.set_connected(1);
    let observer = state.clone();
    let mut sink = TrayWorkerRegistrySink::with_connection_state(Some(sender), state);

    assert_eq!(sink.mark_tray_connection_disconnected(1), Ok(()));

    assert_eq!(
        observer.status(),
        crate::tray::TrayConnectionStatus::Disconnected
    );
}

fn registry() -> TrayActionRegistry {
    TrayActionRegistry::new(vec![
        TrayMenuItem::configured(
            "screenshot-screen",
            "Capture",
            TrayActionKind::CaptureScreenshot,
        )
        .with_enabled(true)
        .with_extension_intent(Some(ExtensionIntentActionId::ScreenshotScreen)),
    ])
}

fn shutdown_channel() -> (
    mpsc::SyncSender<AgentShutdownSignal>,
    mpsc::Receiver<AgentShutdownSignal>,
) {
    mpsc::sync_channel(1)
}

fn apply_registry_command(registry: TrayActionRegistry) -> WorkerCommand {
    let (result_sender, result_receiver) = mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let _ = result_receiver.recv();
    });
    WorkerCommand::ApplyTrayRegistry {
        registry,
        result_sender,
    }
}

fn tray_worker() -> TrayWorker {
    TrayWorker::new(
        AgentStartupStatus::UnsupportedCacheSchema,
        crate::settings::ActiveSettings::extension_managed_default(),
        None,
        super::ManualWorkerSenders::default(),
    )
}

fn command_id_for(worker: &TrayWorker, action_id: &str) -> Result<u32, String> {
    worker
        .commands
        .iter()
        .find(|command| command.action_id == action_id)
        .map(|command| command.command_id)
        .ok_or_else(|| format!("{action_id} command must be rendered"))
}

#[derive(Debug, Default)]
struct FakeTrayCommandSource {
    render_count: u32,
    fail_next_render: bool,
    events: Vec<TrayRuntimeEvent>,
    notices: Vec<TrayNoticeKind>,
    registry_results: Vec<Result<(), TrayRuntimeError>>,
    tooltips: Vec<String>,
    settings_views: Vec<crate::tray::settings_window::model::SettingsWindowView>,
    settings_mode_switches: Vec<bool>,
    confirm_exit: bool,
    exit_confirmations: u32,
    settings_destination: Option<std::path::PathBuf>,
    fail_settings_destination: bool,
}

impl FakeTrayCommandSource {
    fn last_registry_result(&mut self) -> Option<Result<(), TrayRuntimeError>> {
        self.registry_results.pop()
    }
}

impl TrayCommandSource for FakeTrayCommandSource {
    fn apply_render_plan(
        &mut self,
        _plan: &crate::tray::TrayMenuRenderPlan,
        tooltip: &str,
        _icon: crate::tray::TrayIconVariant,
    ) -> Result<(), TrayRuntimeError> {
        if self.fail_next_render {
            self.fail_next_render = false;
            self.registry_results
                .push(Err(TrayRuntimeError::RenderFailed));
            return Err(TrayRuntimeError::RenderFailed);
        }
        self.render_count += 1;
        self.tooltips.push(tooltip.to_owned());
        self.registry_results.push(Ok(()));
        Ok(())
    }

    fn drain_events(&mut self) -> Result<Vec<TrayRuntimeEvent>, TrayRuntimeError> {
        Ok(std::mem::take(&mut self.events))
    }

    fn show_notice(&mut self, kind: TrayNoticeKind) -> Result<(), TrayRuntimeError> {
        self.notices.push(kind);
        Ok(())
    }

    fn confirm_stop_recording_and_exit(&mut self) -> Result<bool, TrayRuntimeError> {
        self.exit_confirmations = self.exit_confirmations.saturating_add(1);
        Ok(self.confirm_exit)
    }

    fn show_settings_window(
        &mut self,
        presentation: crate::tray::settings_window::model::SettingsWindowPresentation,
    ) -> Result<(), TrayRuntimeError> {
        self.settings_views.push(presentation.view());
        self.settings_mode_switches
            .push(presentation.switching_mode());
        Ok(())
    }

    fn hide_settings_window(&mut self) -> Result<(), TrayRuntimeError> {
        Ok(())
    }

    fn pick_settings_destination(
        &mut self,
    ) -> Result<Option<std::path::PathBuf>, TrayRuntimeError> {
        if self.fail_settings_destination {
            return Err(TrayRuntimeError::RenderFailed);
        }
        Ok(self.settings_destination.take())
    }

    fn open_save_folder(&mut self, _destination: &std::path::Path) -> Result<(), TrayRuntimeError> {
        Ok(())
    }
}
