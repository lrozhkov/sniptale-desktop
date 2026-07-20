//! Cursor track serialization for sanitized recording telemetry.

use serde_json::{Value, json};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RecordingTelemetryCursorSample {
    pub(crate) time_ms: u64,
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) visible: bool,
}

impl RecordingTelemetryCursorSample {
    pub(crate) const fn new(time_ms: u64, x: i32, y: i32, visible: bool) -> Self {
        Self {
            time_ms,
            x,
            y,
            visible,
        }
    }
}

pub(super) fn cursor_track_from_samples(samples: Vec<RecordingTelemetryCursorSample>) -> Value {
    json!({
        "captureMode": "embedded-fallback",
        "samples": samples
            .into_iter()
            .enumerate()
            .map(|(index, sample)| {
                json!({
                    "id": format!("native-cursor-{index}"),
                    "time": sample.time_ms,
                    "x": sample.x,
                    "y": sample.y,
                    "visible": sample.visible
                })
            })
            .collect::<Vec<_>>(),
        "skin": {
            "animationPreset": "none",
            "color": "white",
            "hidden": false,
            "preset": "default",
            "scale": 1,
            "shadow": true
        }
    })
}
