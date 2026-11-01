//! Validates retained sanitized telemetry before recovery may send it to a browser.

use crate::protocol::NativeRecordingTelemetrySnapshot;
use serde_json::Value;

pub(crate) fn valid_retained_telemetry(
    snapshot: &NativeRecordingTelemetrySnapshot,
    width: u32,
    height: u32,
    duration_ms: u64,
) -> bool {
    if snapshot.action_events.len() > 2048
        || snapshot.signals.len() > 512
        || !retained_shape(snapshot)
        || serde_json::to_vec(snapshot).map_or(true, |bytes| bytes.len() > 768 * 1024)
    {
        return false;
    }
    let viewport_valid = snapshot.viewport.as_ref().is_some_and(|view| {
        view["width"] == width && view["height"] == height && view["devicePixelRatio"] == 1
    });
    if !viewport_valid {
        return false;
    }
    let point_valid = |point: &Value| {
        point["x"].as_u64().is_some_and(|x| x < u64::from(width))
            && point["y"].as_u64().is_some_and(|y| y < u64::from(height))
    };
    let time_valid = |value: &Value| value.as_u64().is_some_and(|time| time <= duration_ms);
    if !snapshot.action_events.iter().all(|event| {
        time_valid(&event["time"])
            && event["duration"] == 0
            && match event["kind"].as_str() {
                Some("KEY") => {
                    event["point"].is_null()
                        && event["label"]
                            .as_str()
                            .is_some_and(crate::protocol::recording_events::valid_event_key_label)
                }
                Some("CLICK") => {
                    point_valid(&event["point"])
                        && matches!(
                            event["data"]["button"].as_str(),
                            Some("left" | "right" | "middle")
                        )
                }
                _ => false,
            }
    }) {
        return false;
    }
    if !snapshot.signals.iter().all(|signal| {
        time_valid(&signal["startTime"])
            && time_valid(&signal["endTime"])
            && signal["startTime"].as_u64() <= signal["endTime"].as_u64()
            && match signal["kind"].as_str() {
                Some("typing") => {
                    signal["point"].is_null()
                        && signal["data"]["count"]
                            .as_u64()
                            .is_some_and(|count| (1..=65535).contains(&count))
                }
                Some("cursor-idle") => point_valid(&signal["point"]),
                Some("static-frame") => {
                    signal["point"].is_null()
                        && ["source", "code", "severity"].iter().all(|key| {
                            signal["data"][key].as_str().is_some_and(|text| {
                                text.len() <= 64
                                    && text
                                        .bytes()
                                        .all(|byte| byte.is_ascii_lowercase() || byte == b'-')
                            })
                        })
                }
                _ => false,
            }
    }) {
        return false;
    }
    snapshot.cursor_track.as_ref().is_none_or(|track| {
        track["samples"].as_array().is_some_and(|samples| {
            samples.len() <= 4096
                && samples.iter().all(|sample| {
                    time_valid(&sample["time"])
                        && point_valid(sample)
                        && sample["visible"].is_boolean()
                })
        })
    })
}

fn exact_keys(value: &Value, keys: &[&str]) -> bool {
    value.as_object().is_some_and(|object| {
        object.len() == keys.len() && keys.iter().all(|key| object.contains_key(*key))
    })
}

pub(crate) fn browser_delivery_telemetry(
    snapshot: &NativeRecordingTelemetrySnapshot,
    width: u32,
    height: u32,
    duration_ms: u64,
) -> Option<NativeRecordingTelemetrySnapshot> {
    if !valid_retained_telemetry(snapshot, width, height, duration_ms) {
        return None;
    }
    let mut projected = snapshot.clone();
    let viewport = projected.viewport.as_mut()?.as_object_mut()?;
    // Older frame-relative snapshots have no browser scroll. Project without rewriting source state.
    viewport.insert("scrollX".to_owned(), serde_json::json!(0));
    viewport.insert("scrollY".to_owned(), serde_json::json!(0));
    Some(projected)
}

fn frame_viewport_shape(viewport: &Value) -> bool {
    exact_keys(viewport, &["width", "height", "devicePixelRatio"])
        || (exact_keys(
            viewport,
            &["width", "height", "devicePixelRatio", "scrollX", "scrollY"],
        ) && viewport["scrollX"] == 0
            && viewport["scrollY"] == 0)
}

pub(crate) fn legacy_cursor_trajectory_warning(
    snapshot: Option<NativeRecordingTelemetrySnapshot>,
    width: u32,
    height: u32,
    duration_ms: u64,
) -> Option<NativeRecordingTelemetrySnapshot> {
    let mut projected = snapshot
        .filter(|snapshot| valid_retained_telemetry(snapshot, width, height, duration_ms))
        .unwrap_or_else(|| NativeRecordingTelemetrySnapshot {
            viewport: Some(serde_json::json!({"width": width, "height": height,
                "devicePixelRatio": 1, "scrollX": 0, "scrollY": 0})),
            cursor_track: None,
            action_events: Vec::new(),
            signals: Vec::new(),
        });
    projected.cursor_track = None;
    projected
        .action_events
        .retain(|event| event["kind"] == "KEY");
    for (index, event) in projected.action_events.iter_mut().enumerate() {
        event["id"] = serde_json::json!(format!("native-key-{index}"));
    }
    projected.signals.retain(|signal| {
        signal["kind"] != "cursor-idle"
            && !(signal["kind"] == "static-frame"
                && signal["data"]["source"] == "cursor-trajectory")
    });
    if projected.signals.len() > 511 {
        // An inline peer cannot receive the complete history. Declare that format unavailable
        // instead of presenting a thinned history, and preserve every distinct loss warning.
        projected.action_events.clear();
        let mut warnings = Vec::new();
        for signal in projected.signals.drain(..) {
            if signal["kind"] == "static-frame"
                && !warnings
                    .iter()
                    .any(|existing: &Value| existing["data"] == signal["data"])
            {
                warnings.push(signal);
            }
        }
        warnings.push(
            super::snapshot::RecordingTelemetrySignal::telemetry_warning(
                "recording-events",
                "format-unavailable",
                0,
            )
            .into_protocol(0),
        );
        projected.signals = warnings;
    }
    projected.signals.insert(
        0,
        super::snapshot::RecordingTelemetrySignal::telemetry_warning(
            "cursor-trajectory",
            "format-unavailable",
            0,
        )
        .into_protocol(0),
    );
    for (index, signal) in projected.signals.iter_mut().enumerate().skip(1) {
        let prefix = if signal["kind"] == "typing" {
            "native-typing"
        } else {
            "native-static"
        };
        signal["id"] = serde_json::json!(format!("{prefix}-{index}"));
    }
    valid_retained_telemetry(&projected, width, height, duration_ms).then_some(projected)
}

pub(crate) fn legacy_recording_events_warning(
    snapshot: Option<NativeRecordingTelemetrySnapshot>,
    width: u32,
    height: u32,
    duration_ms: u64,
) -> Option<NativeRecordingTelemetrySnapshot> {
    let mut projected = snapshot
        .filter(|value| valid_retained_telemetry(value, width, height, duration_ms))
        .unwrap_or_else(|| NativeRecordingTelemetrySnapshot {
            viewport: Some(serde_json::json!({
            "width": width, "height": height, "devicePixelRatio": 1, "scrollX": 0, "scrollY": 0})),
            cursor_track: None,
            action_events: Vec::new(),
            signals: Vec::new(),
        });
    projected.action_events.clear();
    let mut warnings = Vec::new();
    for signal in projected.signals.drain(..) {
        if signal["kind"] == "static-frame"
            && signal["data"]["source"] != "recording-events"
            && !warnings
                .iter()
                .any(|existing: &Value| existing["data"] == signal["data"])
        {
            warnings.push(signal);
        }
    }
    warnings.push(
        super::snapshot::RecordingTelemetrySignal::telemetry_warning(
            "recording-events",
            "format-unavailable",
            0,
        )
        .into_protocol(0),
    );
    for (index, signal) in warnings.iter_mut().enumerate() {
        signal["id"] = serde_json::json!(format!("native-static-{index}"));
    }
    projected.signals = warnings;
    valid_retained_telemetry(&projected, width, height, duration_ms).then_some(projected)
}

fn retained_shape(snapshot: &NativeRecordingTelemetrySnapshot) -> bool {
    snapshot.viewport.as_ref().is_some_and(frame_viewport_shape)
        && snapshot
            .action_events
            .iter()
            .enumerate()
            .all(|(index, event)| {
                exact_keys(
                    event,
                    &[
                        "id", "kind", "label", "time", "duration", "point", "data", "preset",
                    ],
                ) && event["preset"] == "NONE"
                    && match event["kind"].as_str() {
                        Some("KEY") => {
                            event["id"] == format!("native-key-{index}")
                                && exact_keys(&event["data"], &[])
                        }
                        Some("CLICK") => {
                            event["id"] == format!("native-click-{index}")
                                && event["label"] == "Click"
                                && exact_keys(&event["point"], &["x", "y"])
                                && exact_keys(&event["data"], &["button"])
                        }
                        _ => false,
                    }
            })
        && snapshot.signals.iter().enumerate().all(|(index, signal)| {
            exact_keys(
                signal,
                &["id", "kind", "startTime", "endTime", "point", "data"],
            ) && match signal["kind"].as_str() {
                Some("typing") => {
                    signal["id"] == format!("native-typing-{index}")
                        && exact_keys(&signal["data"], &["count"])
                }
                Some("cursor-idle") => {
                    signal["id"] == format!("native-cursor-idle-{index}")
                        && exact_keys(&signal["point"], &["x", "y"])
                        && signal["data"] == serde_json::json!({"source": "native-cursor"})
                }
                Some("static-frame") => {
                    signal["id"] == format!("native-static-{index}")
                        && retained_warning(&signal["data"])
                }
                _ => false,
            }
        })
        && snapshot
            .cursor_track
            .as_ref()
            .is_none_or(retained_cursor_shape)
}

fn retained_warning(data: &Value) -> bool {
    exact_keys(data, &["source", "code", "severity"])
        && data["severity"] == "warning"
        && match data["source"].as_str() {
            Some("keyboard-hook" | "mouse-hook") => matches!(
                data["code"].as_str(),
                Some(
                    "events-dropped"
                        | "drain-failed"
                        | "signal-record-failed"
                        | "event-record-failed"
                        | "blocked"
                        | "permission-denied"
                        | "policy-denied"
                        | "install-failed"
                )
            ),
            Some("cursor-telemetry") => data["code"] == "idle-signal-record-failed",
            Some("cursor-trajectory" | "recording-events") => data["code"] == "format-unavailable",
            _ => false,
        }
}

fn retained_cursor_shape(track: &Value) -> bool {
    exact_keys(track, &["captureMode", "samples", "skin"])
        && track["captureMode"] == "embedded-fallback"
        && track["skin"]
            == serde_json::json!({"animationPreset": "none", "color": "white", "hidden": false, "preset": "default", "scale": 1, "shadow": true})
        && track["samples"].as_array().is_some_and(|samples| {
            samples.iter().enumerate().all(|(index, sample)| {
                exact_keys(sample, &["id", "time", "x", "y", "visible"])
                    && sample["id"] == format!("native-cursor-{index}")
            })
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retained_key_labels_reject_typed_text_and_accept_only_sanitized_keys_or_shortcuts() {
        assert!(crate::protocol::recording_events::valid_event_key_label(
            "Ctrl+Shift+A"
        ));
        assert!(crate::protocol::recording_events::valid_event_key_label(
            "Enter"
        ));
        assert!(crate::protocol::recording_events::valid_event_key_label(
            "F24"
        ));
        for label in ["a", "secret-text", "Shift+A", "Ctrl+password", "F999"] {
            assert!(!crate::protocol::recording_events::valid_event_key_label(
                label
            ));
        }
    }

    #[test]
    fn legacy_history_format_warnings_have_independent_stable_signal_ids()
    -> Result<(), Box<dyn std::error::Error>> {
        let cursor = legacy_cursor_trajectory_warning(None, 100, 80, 20);
        let both = legacy_recording_events_warning(cursor, 100, 80, 20).ok_or("warnings")?;
        assert_eq!(both.signals.len(), 2);
        assert_eq!(both.signals[0]["id"], "native-static-0");
        assert_eq!(both.signals[0]["data"]["source"], "cursor-trajectory");
        assert_eq!(both.signals[1]["id"], "native-static-1");
        assert_eq!(both.signals[1]["data"]["source"], "recording-events");
        assert!(valid_retained_telemetry(&both, 100, 80, 20));
        Ok(())
    }

    fn retained_snapshot() -> NativeRecordingTelemetrySnapshot {
        use crate::telemetry::{
            SanitizedRecordingTelemetrySnapshot,
            frame::RecordingFrameGeometry,
            snapshot::{
                RecordingTelemetryActionEvent, RecordingTelemetryCursorSample,
                RecordingTelemetrySignal,
            },
        };
        SanitizedRecordingTelemetrySnapshot::new(
            vec![
                RecordingTelemetryActionEvent::Key {
                    label: "Ctrl+A".to_owned(),
                    time_ms: 5,
                },
                RecordingTelemetryActionEvent::click("left", 10, 20, 30),
            ],
            vec![
                RecordingTelemetrySignal::typing_span(2, 5, 10),
                RecordingTelemetrySignal::cursor_idle(5, 10, 20, 30),
                RecordingTelemetrySignal::telemetry_warning("mouse-hook", "events-dropped", 10),
            ],
        )
        .with_cursor_samples(vec![RecordingTelemetryCursorSample::new(10, 20, 30, true)])
        .for_frame(RecordingFrameGeometry {
            x: 0,
            y: 0,
            width: 100,
            height: 80,
        })
        .into_protocol()
    }

    #[test]
    fn legacy_projection_preserves_loss_at_the_last_slot_and_declares_history_unavailable()
    -> Result<(), Box<dyn std::error::Error>> {
        use crate::telemetry::snapshot::RecordingTelemetrySignal;
        let mut snapshot = retained_snapshot();
        snapshot.signals = (0..511)
            .map(|index| RecordingTelemetrySignal::typing_span(1, 0, 1).into_protocol(index))
            .collect();
        snapshot.signals.push(
            RecordingTelemetrySignal::telemetry_warning("keyboard-hook", "events-dropped", 1)
                .into_protocol(511),
        );
        assert!(valid_retained_telemetry(&snapshot, 100, 80, 20));
        let source = snapshot.clone();
        let projected =
            legacy_cursor_trajectory_warning(Some(snapshot), 100, 80, 20).ok_or("projection")?;
        assert_eq!(projected.action_events, Vec::<serde_json::Value>::new());
        assert_eq!(projected.signals.len(), 3);
        for (source, code) in [
            ("keyboard-hook", "events-dropped"),
            ("recording-events", "format-unavailable"),
            ("cursor-trajectory", "format-unavailable"),
        ] {
            assert!(
                projected
                    .signals
                    .iter()
                    .any(|signal| signal["data"]["source"] == source
                        && signal["data"]["code"] == code)
            );
        }
        assert_eq!(source.signals.len(), 512);
        assert_eq!(source.action_events.len(), 2);
        Ok(())
    }

    #[test]
    fn browser_projection_supplies_required_zero_scroll_without_mutating_legacy_source()
    -> Result<(), Box<dyn std::error::Error>> {
        let canonical = retained_snapshot();
        let projected = browser_delivery_telemetry(&canonical, 100, 80, 20).ok_or("canonical")?;
        assert_eq!(projected.viewport.as_ref().ok_or("viewport")?["scrollX"], 0);
        assert_eq!(projected.viewport.as_ref().ok_or("viewport")?["scrollY"], 0);
        let mut legacy = canonical.clone();
        let viewport = legacy
            .viewport
            .as_mut()
            .and_then(Value::as_object_mut)
            .ok_or("viewport")?;
        viewport.remove("scrollX");
        viewport.remove("scrollY");
        let original = legacy.clone();
        assert_eq!(
            browser_delivery_telemetry(&legacy, 100, 80, 20),
            Some(projected)
        );
        assert_eq!(legacy, original);
        for (key, value) in [
            ("scrollX", serde_json::json!(1)),
            ("scrollY", serde_json::json!("0")),
            ("url", serde_json::json!("private")),
        ] {
            let mut invalid = canonical.clone();
            invalid.viewport.as_mut().ok_or("viewport")?[key] = value;
            assert!(browser_delivery_telemetry(&invalid, 100, 80, 20).is_none());
        }
        let mut partial = canonical;
        partial
            .viewport
            .as_mut()
            .and_then(Value::as_object_mut)
            .ok_or("viewport")?
            .remove("scrollX");
        assert!(browser_delivery_telemetry(&partial, 100, 80, 20).is_none());
        assert!(browser_delivery_telemetry(&legacy, 101, 80, 20).is_none());
        assert!(browser_delivery_telemetry(&legacy, 100, 80, 9).is_none());
        Ok(())
    }

    #[test]
    fn recovery_rejects_unknown_fields_and_arbitrary_text_in_nested_telemetry()
    -> Result<(), Box<dyn std::error::Error>> {
        let valid = retained_snapshot();
        assert!(valid_retained_telemetry(&valid, 100, 80, 20));
        for field in [
            "viewport",
            "event",
            "point",
            "data",
            "signal",
            "warning",
            "skin",
            "sample",
            "event-id",
            "signal-id",
            "sample-id",
            "click-label",
            "warning-code",
        ] {
            let mut snapshot = valid.clone();
            let target = match field {
                "viewport" => &mut snapshot.viewport.as_mut().ok_or("viewport")?["extra"],
                "event" => &mut snapshot.action_events[0]["extra"],
                "point" => &mut snapshot.action_events[1]["point"]["extra"],
                "data" => &mut snapshot.action_events[0]["data"]["extra"],
                "signal" => &mut snapshot.signals[0]["extra"],
                "warning" => &mut snapshot.signals[2]["data"]["extra"],
                "skin" => &mut snapshot.cursor_track.as_mut().ok_or("cursor")?["skin"]["extra"],
                "sample" => {
                    &mut snapshot.cursor_track.as_mut().ok_or("cursor")?["samples"][0]["extra"]
                }
                "event-id" => &mut snapshot.action_events[0]["id"],
                "signal-id" => &mut snapshot.signals[0]["id"],
                "sample-id" => {
                    &mut snapshot.cursor_track.as_mut().ok_or("cursor")?["samples"][0]["id"]
                }
                "click-label" => &mut snapshot.action_events[1]["label"],
                "warning-code" => &mut snapshot.signals[2]["data"]["code"],
                _ => return Err("unknown mutation".into()),
            };
            *target = serde_json::json!("untrusted-sensitive-text");
            assert!(!valid_retained_telemetry(&snapshot, 100, 80, 20), "{field}");
        }
        Ok(())
    }
}
