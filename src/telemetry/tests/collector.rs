use super::super::{
    RecordingTelemetryCollector, TelemetryCollectorError,
    sanitize::{KeyCode, KeyModifiers},
    snapshot::RecordingTelemetrySource,
};

#[test]
fn collector_compacts_typed_characters_without_raw_text() -> Result<(), TelemetryCollectorError> {
    let mut collector = RecordingTelemetryCollector::default();
    collector.start_recording("recording-1", 1_000);

    collector.record_key(
        "recording-1",
        KeyCode::Character('x'),
        KeyModifiers::default(),
        1_001_000,
    )?;
    collector.record_key(
        "recording-1",
        KeyCode::Character('q'),
        KeyModifiers::default(),
        3_001_000,
    )?;

    let snapshot = collector
        .snapshot_for_recording("recording-1", 5)
        .ok_or(TelemetryCollectorError::NotRecording)?
        .into_protocol();

    assert!(snapshot.action_events.is_empty());
    assert_eq!(snapshot.signals.len(), 1);
    assert_eq!(snapshot.signals[0]["kind"], "typing");
    assert_eq!(snapshot.signals[0]["startTime"], 1);
    assert_eq!(snapshot.signals[0]["endTime"], 5);
    assert_eq!(snapshot.signals[0]["data"]["count"], 2);
    let serialized = serde_json::to_string(&snapshot.signals)
        .map_err(|_| TelemetryCollectorError::NotRecording)?;
    assert!(!serialized.contains("\"x\""));
    assert!(!serialized.contains("\"q\""));
    Ok(())
}

#[test]
fn collector_flushes_typing_before_safe_shortcut() -> Result<(), TelemetryCollectorError> {
    let mut collector = RecordingTelemetryCollector::default();
    collector.start_recording("recording-1", 0);

    collector.record_key(
        "recording-1",
        KeyCode::Character('x'),
        KeyModifiers::default(),
        1_000_000,
    )?;
    collector.record_key(
        "recording-1",
        KeyCode::Character('s'),
        KeyModifiers::CTRL,
        2_000_000,
    )?;
    collector.record_key(
        "recording-1",
        KeyCode::Character('q'),
        KeyModifiers::default(),
        3_000_000,
    )?;

    let snapshot = collector
        .snapshot_for_recording("recording-1", 4)
        .ok_or(TelemetryCollectorError::NotRecording)?
        .into_protocol();

    assert_eq!(snapshot.action_events.len(), 1);
    assert_eq!(snapshot.action_events[0]["label"], "Ctrl+S");
    assert_eq!(snapshot.action_events[0]["time"], 2);
    assert_eq!(snapshot.signals.len(), 2);
    assert_eq!(snapshot.signals[0]["startTime"], 1);
    assert_eq!(snapshot.signals[0]["endTime"], 1);
    assert_eq!(snapshot.signals[1]["startTime"], 3);
    assert_eq!(snapshot.signals[1]["endTime"], 4);
    Ok(())
}

#[test]
fn collector_compacts_paused_intervals_into_media_time() -> Result<(), TelemetryCollectorError> {
    let mut collector = RecordingTelemetryCollector::default();
    collector.start_recording("recording-1", 1_000);

    collector.record_key(
        "recording-1",
        KeyCode::Character('x'),
        KeyModifiers::default(),
        2_000_000,
    )?;
    collector.record_pause("recording-1", 3_000_000, 8_000_000)?;
    collector.record_key(
        "recording-1",
        KeyCode::Character('s'),
        KeyModifiers::CTRL,
        10_000_000,
    )?;

    let snapshot = collector
        .snapshot_for_recording("recording-1", 5)
        .ok_or(TelemetryCollectorError::NotRecording)?
        .into_protocol();

    assert_eq!(snapshot.signals[0]["startTime"], 1);
    assert_eq!(snapshot.signals[0]["endTime"], 1);
    assert_eq!(snapshot.action_events[0]["time"], 4);
    Ok(())
}

#[test]
fn collector_closes_open_pause_from_lifecycle_callbacks() -> Result<(), TelemetryCollectorError> {
    let mut collector = RecordingTelemetryCollector::default();
    collector.start_recording("recording-1", 0);

    collector.record_key(
        "recording-1",
        KeyCode::Character('x'),
        KeyModifiers::default(),
        2_000_000,
    )?;
    collector.pause_recording("recording-1", 3_000_000);
    collector.resume_recording("recording-1", 8_000_000);
    collector.record_key(
        "recording-1",
        KeyCode::Character('s'),
        KeyModifiers::CTRL,
        10_000_000,
    )?;

    let snapshot = collector
        .snapshot_for_recording("recording-1", 6)
        .ok_or(TelemetryCollectorError::NotRecording)?
        .into_protocol();

    assert_eq!(snapshot.signals[0]["startTime"], 2);
    assert_eq!(snapshot.signals[0]["endTime"], 2);
    assert_eq!(snapshot.action_events[0]["time"], 5);
    Ok(())
}

#[test]
fn collector_rejects_mismatched_recording_without_dropping_session() {
    let mut collector = RecordingTelemetryCollector::default();

    assert_eq!(
        collector.record_key(
            "recording-1",
            KeyCode::Enter,
            KeyModifiers::default(),
            1_000_000,
        ),
        Err(TelemetryCollectorError::NotRecording)
    );

    collector.start_recording("recording-1", 0);
    assert_eq!(
        collector.record_key(
            "recording-2",
            KeyCode::Enter,
            KeyModifiers::default(),
            1_000_000,
        ),
        Err(TelemetryCollectorError::RecordingMismatch)
    );
    assert!(collector.snapshot_for_recording("recording-2", 1).is_none());

    assert_eq!(
        collector.record_key(
            "recording-1",
            KeyCode::Enter,
            KeyModifiers::default(),
            1_000_000,
        ),
        Ok(())
    );
    assert!(collector.snapshot_for_recording("recording-1", 1).is_some());
}
