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

    assert_eq!(snapshot.action_events.len(), 0);
    assert_eq!(snapshot.signals.len(), 1);
    assert_eq!(snapshot.signals[0]["kind"], "typing");
    assert_eq!(snapshot.signals[0]["startTime"], 1);
    assert_eq!(snapshot.signals[0]["endTime"], 3);
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
    assert_eq!(snapshot.signals[1]["endTime"], 3);
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
    collector.pause_recording("recording-1", 3_000_000);
    collector.resume_recording("recording-1", 8_000_000);
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

#[test]
fn typing_counter_emits_complete_spans_instead_of_saturating_and_stop_does_not_extend_idle_time()
-> Result<(), TelemetryCollectorError> {
    use crate::telemetry::sanitize::SanitizedKeySignal;
    let mut collector = RecordingTelemetryCollector::default();
    collector.start_recording("recording-typing", 0);
    for sequence in 0..70_000_u64 {
        collector.record_signal(
            "recording-typing",
            SanitizedKeySignal::Typing { count: 1 },
            sequence * 1_000_000,
        )?;
    }
    let snapshot = collector
        .snapshot_for_recording("recording-typing", 600_000)
        .ok_or(TelemetryCollectorError::NotRecording)?
        .into_protocol();
    assert_eq!(snapshot.signals.len(), 2);
    let counts: Vec<_> = snapshot
        .signals
        .iter()
        .filter_map(|signal| signal["data"]["count"].as_u64())
        .collect();
    assert_eq!(counts, [65535, 4465]);
    assert_eq!(counts.into_iter().sum::<u64>(), 70_000);
    assert_eq!(snapshot.signals[0]["endTime"], 65534);
    assert_eq!(snapshot.signals[1]["startTime"], 65535);
    assert_eq!(snapshot.signals[1]["endTime"], 69999);
    Ok(())
}

#[test]
fn bounded_drains_preserve_five_thousand_actions_without_a_lifetime_limit()
-> Result<(), TelemetryCollectorError> {
    let mut collector = RecordingTelemetryCollector::default();
    collector.start_recording("history", 0);
    let mut received = 0_u64;
    for sequence in 0..5000_u64 {
        collector.record_key(
            "history",
            KeyCode::Enter,
            KeyModifiers::default(),
            sequence * 1_000_000,
        )?;
        if sequence % 257 == 256 || sequence == 4999 {
            let batch = collector.drain_pending_for_recording("history")?;
            assert!(!batch.history_incomplete);
            let batch = batch.into_protocol();
            assert!(batch.action_events.len() <= 257);
            for event in batch.action_events {
                assert_eq!(event["time"], received);
                received += 1;
            }
        }
    }
    assert_eq!(received, 5000);
    Ok(())
}

#[test]
fn backpressure_preserves_the_accepted_prefix_and_sticky_loss_across_drains()
-> Result<(), TelemetryCollectorError> {
    let mut collector = RecordingTelemetryCollector::default();
    collector.start_recording("history", 0);
    for sequence in 0..2048_u64 {
        collector.record_key(
            "history",
            KeyCode::Enter,
            KeyModifiers::default(),
            sequence * 1_000_000,
        )?;
    }
    assert_eq!(
        collector.record_key(
            "history",
            KeyCode::Enter,
            KeyModifiers::default(),
            2_048_000_000
        ),
        Err(TelemetryCollectorError::HistoryBackpressure)
    );
    let batch = collector.drain_pending_for_recording("history")?;
    assert!(batch.history_incomplete);
    let batch = batch.into_protocol();
    assert_eq!(batch.action_events.len(), 2048);
    for (index, event) in batch.action_events.iter().enumerate() {
        assert_eq!(event["time"], index);
    }
    assert_eq!(
        batch.signals.last().map(|signal| &signal["data"]["code"]),
        Some(&serde_json::json!("events-dropped"))
    );
    collector.record_key(
        "history",
        KeyCode::Enter,
        KeyModifiers::default(),
        2_049_000_000,
    )?;
    let next = collector.drain_pending_for_recording("history")?;
    assert!(next.history_incomplete);
    assert_eq!(next.into_protocol().action_events[0]["time"], 2049);
    Ok(())
}
