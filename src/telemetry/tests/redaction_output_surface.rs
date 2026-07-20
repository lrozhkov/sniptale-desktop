use super::super::{
    RecordingTelemetryCollector, TelemetryCollectorError,
    sanitize::{KeyCode, KeyModifiers},
    snapshot::RecordingTelemetrySource,
};

#[test]
fn telemetry_output_never_contains_typed_text_clipboard_or_url()
-> Result<(), TelemetryCollectorError> {
    let mut collector = RecordingTelemetryCollector::default();
    collector.start_recording("recording-1", 1_000);
    for (index, character) in "secret".chars().enumerate() {
        collector.record_key(
            "recording-1",
            KeyCode::Character(character),
            KeyModifiers::default(),
            1_001_000 + u64::try_from(index).unwrap_or(0),
        )?;
    }

    let snapshot = collector
        .snapshot_for_recording("recording-1", 5)
        .ok_or(TelemetryCollectorError::NotRecording)?
        .into_protocol();
    let serialized =
        serde_json::to_string(&snapshot).map_err(|_| TelemetryCollectorError::NotRecording)?;

    for forbidden in [
        "secret",
        "clipboard",
        "https://sniptale.invalid/private",
        "http://sniptale.invalid/private",
    ] {
        assert!(
            !serialized.contains(forbidden),
            "telemetry output leaked forbidden value {forbidden}"
        );
    }
    Ok(())
}
