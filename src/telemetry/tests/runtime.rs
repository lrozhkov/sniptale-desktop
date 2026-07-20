use super::super::{
    TelemetryCollectorError,
    policy::RecordingTelemetryPolicy,
    runtime::RuntimeRecordingTelemetrySource,
    snapshot::RecordingTelemetrySource,
    windows::{
        WindowsKeyEvent, WindowsKeyModifiers, WindowsMouseButton, WindowsMouseEvent,
        WindowsMouseEventKind,
    },
};

#[test]
fn runtime_telemetry_source_produces_sanitized_snapshot() -> Result<(), TelemetryCollectorError> {
    let mut source = RuntimeRecordingTelemetrySource::default();
    source.configure_recording(RecordingTelemetryPolicy::all_enabled_for_tests());
    source.start_recording("recording-1", 1_000);
    source.inject_key_event(
        "recording-1",
        WindowsKeyEvent {
            virtual_key: 0x58,
            modifiers: WindowsKeyModifiers::default(),
        },
        2_001_000,
    )?;
    source.inject_key_event(
        "recording-1",
        WindowsKeyEvent {
            virtual_key: 0x53,
            modifiers: WindowsKeyModifiers::CTRL,
        },
        3_001_000,
    )?;
    source.finish_recording("recording-1", 4_001_000);

    let snapshot = source
        .snapshot_for_recording("recording-1", 5)
        .ok_or(TelemetryCollectorError::NotRecording)?
        .into_protocol();

    assert_eq!(snapshot.signals.len(), 1);
    assert_eq!(snapshot.signals[0]["kind"], "typing");
    assert_eq!(snapshot.signals[0]["data"]["count"], 1);
    assert_eq!(snapshot.action_events.len(), 1);
    assert_eq!(snapshot.action_events[0]["label"], "Ctrl+S");
    let serialized =
        serde_json::to_string(&snapshot).map_err(|_| TelemetryCollectorError::NotRecording)?;
    assert!(!serialized.contains('x'));
    assert!(!serialized.contains('X'));
    Ok(())
}

#[test]
fn runtime_telemetry_source_records_cursor_click_and_static_signal()
-> Result<(), TelemetryCollectorError> {
    let mut source = RuntimeRecordingTelemetrySource::default();
    source.configure_recording(RecordingTelemetryPolicy::all_enabled_for_tests());
    source.start_recording("recording-1", 1_000);
    source.inject_mouse_event(
        "recording-1",
        WindowsMouseEvent {
            x: 42,
            y: 84,
            kind: WindowsMouseEventKind::Move,
        },
        2_001_000,
    )?;
    source.inject_mouse_event(
        "recording-1",
        WindowsMouseEvent {
            x: 45,
            y: 88,
            kind: WindowsMouseEventKind::Click(WindowsMouseButton::Left),
        },
        3_001_000,
    )?;
    source.finish_recording("recording-1", 4_001_000);

    let snapshot = source
        .snapshot_for_recording("recording-1", 5)
        .ok_or(TelemetryCollectorError::NotRecording)?
        .into_protocol();

    assert_eq!(
        snapshot
            .cursor_track
            .as_ref()
            .and_then(|track| track["samples"][0]["x"].as_i64()),
        Some(42)
    );
    assert_eq!(snapshot.action_events.len(), 1);
    assert_eq!(snapshot.action_events[0]["kind"], "CLICK");
    assert_eq!(snapshot.action_events[0]["data"]["button"], "left");
    assert!(
        snapshot
            .signals
            .iter()
            .any(|signal| signal["kind"] == "cursor-idle")
    );
    Ok(())
}

#[test]
fn runtime_telemetry_source_denies_events_by_default() -> Result<(), TelemetryCollectorError> {
    let mut source = RuntimeRecordingTelemetrySource::default();
    source.start_recording("recording-1", 1_000);
    source.inject_key_event(
        "recording-1",
        WindowsKeyEvent {
            virtual_key: 0x58,
            modifiers: WindowsKeyModifiers::default(),
        },
        2_001_000,
    )?;
    source.inject_mouse_event(
        "recording-1",
        WindowsMouseEvent {
            x: 42,
            y: 84,
            kind: WindowsMouseEventKind::Click(WindowsMouseButton::Left),
        },
        3_001_000,
    )?;
    source.finish_recording("recording-1", 4_001_000);

    let snapshot = source
        .snapshot_for_recording("recording-1", 5)
        .ok_or(TelemetryCollectorError::NotRecording)?
        .into_protocol();

    assert_eq!(snapshot.cursor_track, None);
    assert!(snapshot.action_events.is_empty());
    assert!(snapshot.signals.is_empty());
    Ok(())
}

#[test]
fn runtime_telemetry_source_rejects_events_outside_active_recording() {
    let mut source = RuntimeRecordingTelemetrySource::default();
    source.configure_recording(RecordingTelemetryPolicy::all_enabled_for_tests());

    assert_eq!(
        source.inject_key_event(
            "recording-1",
            WindowsKeyEvent {
                virtual_key: 0x58,
                modifiers: WindowsKeyModifiers::default(),
            },
            2_001_000,
        ),
        Err(TelemetryCollectorError::NotRecording)
    );

    source.start_recording("recording-1", 1_000);
    assert_eq!(
        source.inject_mouse_event(
            "recording-2",
            WindowsMouseEvent {
                x: 42,
                y: 84,
                kind: WindowsMouseEventKind::Move,
            },
            2_001_000,
        ),
        Err(TelemetryCollectorError::NotRecording)
    );
}

#[test]
fn runtime_telemetry_source_ignores_mid_session_policy_enable()
-> Result<(), TelemetryCollectorError> {
    let mut source = RuntimeRecordingTelemetrySource::default();
    source.start_recording("recording-1", 1_000);
    source.configure_recording(RecordingTelemetryPolicy::all_enabled_for_tests());
    source.inject_key_event(
        "recording-1",
        WindowsKeyEvent {
            virtual_key: 0x58,
            modifiers: WindowsKeyModifiers::default(),
        },
        2_001_000,
    )?;
    source.finish_recording("recording-1", 3_001_000);

    let snapshot = source
        .snapshot_for_recording("recording-1", 4)
        .ok_or(TelemetryCollectorError::NotRecording)?
        .into_protocol();

    assert!(snapshot.signals.is_empty());
    assert!(snapshot.action_events.is_empty());
    Ok(())
}

#[test]
fn runtime_telemetry_source_records_typed_hook_warning() -> Result<(), TelemetryCollectorError> {
    let mut source = RuntimeRecordingTelemetrySource::default();
    source.start_recording("recording-1", 1_000);
    source.inject_telemetry_warning("recording-1", "keyboard-hook", "install-failed", 2_001_000)?;
    source.finish_recording("recording-1", 3_001_000);

    let snapshot = source
        .snapshot_for_recording("recording-1", 5)
        .ok_or(TelemetryCollectorError::NotRecording)?
        .into_protocol();

    assert_eq!(snapshot.signals.len(), 1);
    assert_eq!(snapshot.signals[0]["kind"], "static-frame");
    assert_eq!(snapshot.signals[0]["data"]["source"], "keyboard-hook");
    assert_eq!(snapshot.signals[0]["data"]["code"], "install-failed");
    Ok(())
}

#[test]
fn runtime_telemetry_source_honors_telemetry_policy_flags() -> Result<(), TelemetryCollectorError> {
    let mut source = RuntimeRecordingTelemetrySource::default();
    source.configure_recording(RecordingTelemetryPolicy {
        collect_cursor: false,
        collect_clicks: false,
        collect_key_events: false,
        collect_typing_spans: true,
        collect_static_signals: false,
        telemetry_non_fatal: true,
    });
    source.start_recording("recording-1", 1_000);
    source.inject_key_event(
        "recording-1",
        WindowsKeyEvent {
            virtual_key: 0x58,
            modifiers: WindowsKeyModifiers::default(),
        },
        2_001_000,
    )?;
    source.inject_key_event(
        "recording-1",
        WindowsKeyEvent {
            virtual_key: 0x53,
            modifiers: WindowsKeyModifiers::CTRL,
        },
        3_001_000,
    )?;
    source.inject_mouse_event(
        "recording-1",
        WindowsMouseEvent {
            x: 42,
            y: 84,
            kind: WindowsMouseEventKind::Click(WindowsMouseButton::Left),
        },
        4_001_000,
    )?;
    source.finish_recording("recording-1", 5_001_000);

    let snapshot = source
        .snapshot_for_recording("recording-1", 6)
        .ok_or(TelemetryCollectorError::NotRecording)?
        .into_protocol();

    assert_eq!(snapshot.cursor_track, None);
    assert!(snapshot.action_events.is_empty());
    assert_eq!(snapshot.signals.len(), 1);
    assert_eq!(snapshot.signals[0]["kind"], "typing");
    Ok(())
}
