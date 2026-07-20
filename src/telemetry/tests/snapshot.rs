use super::super::{
    sanitize::{KeyCode, KeyModifiers, sanitize_key},
    snapshot::{
        RecordingTelemetryActionEvent, RecordingTelemetrySignal,
        SanitizedRecordingTelemetrySnapshot,
    },
};

#[test]
fn sanitized_snapshot_uses_extension_project_telemetry_shape() -> Result<(), &'static str> {
    let action = RecordingTelemetryActionEvent::from_key_signal(
        sanitize_key(KeyCode::Character('s'), KeyModifiers::CTRL),
        12,
    )
    .ok_or("shortcut should produce a telemetry action")?;
    let snapshot = SanitizedRecordingTelemetrySnapshot::new(
        vec![action],
        vec![RecordingTelemetrySignal::typing_span(3, 10, 30)],
    )
    .into_protocol();

    let action = &snapshot.action_events[0];
    assert_eq!(action["id"], "native-key-0");
    assert_eq!(action["kind"], "KEY");
    assert_eq!(action["time"], 12);
    assert_eq!(action["duration"], 0);
    assert_eq!(action["point"], serde_json::Value::Null);
    assert_eq!(action["label"], "Ctrl+S");
    assert_eq!(action["preset"], "NONE");
    assert!(action["data"].is_object());

    let signal = &snapshot.signals[0];
    assert_eq!(signal["id"], "native-typing-0");
    assert_eq!(signal["kind"], "typing");
    assert_eq!(signal["startTime"], 10);
    assert_eq!(signal["endTime"], 30);
    assert_eq!(signal["point"], serde_json::Value::Null);
    assert_eq!(signal["data"]["count"], 3);
    Ok(())
}

#[test]
fn sanitized_snapshot_can_carry_safe_viewport_and_cursor_track() {
    let snapshot = SanitizedRecordingTelemetrySnapshot::new(Vec::new(), Vec::new())
        .with_viewport(serde_json::json!({
            "width": 1920,
            "height": 1080,
            "scrollX": 0,
            "scrollY": 0,
            "devicePixelRatio": 1
        }))
        .with_cursor_track(serde_json::json!({
            "captureMode": "embedded-fallback",
            "samples": [
                {
                    "id": "native-cursor-0",
                    "time": 12,
                    "x": 100,
                    "y": 200,
                    "visible": true
                }
            ],
            "skin": {
                "color": "white",
                "hidden": false,
                "scale": 1,
                "shadow": true
            }
        }))
        .into_protocol();

    assert_eq!(
        snapshot
            .viewport
            .as_ref()
            .and_then(|value| value["width"].as_u64()),
        Some(1920)
    );
    assert_eq!(
        snapshot
            .cursor_track
            .as_ref()
            .and_then(|value| value["samples"][0]["id"].as_str()),
        Some("native-cursor-0")
    );
}
