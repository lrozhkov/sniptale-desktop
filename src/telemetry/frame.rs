//! Physical desktop points narrowed into the recorded frame's pixel coordinate space.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RecordingPointMapping {
    FixedDesktopFrame,
    Unavailable,
}

impl RecordingPointMapping {
    pub(crate) const fn for_capture_mode(mode: &crate::protocol::VideoMode) -> Self {
        match mode {
            crate::protocol::VideoMode::Screen | crate::protocol::VideoMode::Region => {
                Self::FixedDesktopFrame
            }
            // Initial window bounds cannot map later input after that window moves.
            crate::protocol::VideoMode::ActiveWindow => Self::Unavailable,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RecordingFrameGeometry {
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

impl RecordingFrameGeometry {
    pub(crate) fn point(self, x: i32, y: i32) -> Option<(i32, i32)> {
        let x = x.checked_sub(self.x)?;
        let y = y.checked_sub(self.y)?;
        if x < 0
            || y < 0
            || u32::try_from(x).ok()? >= self.width
            || u32::try_from(y).ok()? >= self.height
        {
            return None;
        }
        Some((x, y))
    }
}

impl super::SanitizedRecordingTelemetrySnapshot {
    pub(crate) fn for_finalized_media(
        self,
        frame: Option<RecordingFrameGeometry>,
        width: u32,
        height: u32,
        duration_ms: u64,
    ) -> Option<crate::protocol::NativeRecordingTelemetrySnapshot> {
        use super::snapshot::{RecordingTelemetryActionEvent, RecordingTelemetrySignal};
        let mut snapshot = self.for_frame(frame?);
        snapshot.action_events.retain(|event| match event {
            RecordingTelemetryActionEvent::Key { time_ms, .. }
            | RecordingTelemetryActionEvent::Click { time_ms, .. } => *time_ms <= duration_ms,
        });
        snapshot.signals.retain_mut(|signal| {
            let (start, end) = match signal {
                RecordingTelemetrySignal::TypingSpan {
                    start_time_ms,
                    end_time_ms,
                    ..
                }
                | RecordingTelemetrySignal::CursorIdle {
                    start_time_ms,
                    end_time_ms,
                    ..
                }
                | RecordingTelemetrySignal::StaticFrame {
                    start_time_ms,
                    end_time_ms,
                    ..
                } => (start_time_ms, end_time_ms),
            };
            *end = (*end).min(duration_ms);
            *start <= *end
        });
        if let Some(samples) = snapshot
            .cursor_track
            .as_mut()
            .and_then(|track| track["samples"].as_array_mut())
        {
            samples.retain(|sample| {
                sample["time"]
                    .as_u64()
                    .is_some_and(|time| time <= duration_ms)
            });
            for (index, sample) in samples.iter_mut().enumerate() {
                sample["id"] = serde_json::json!(format!("native-cursor-{index}"));
            }
        }
        let snapshot = snapshot.into_protocol();
        super::valid_retained_telemetry(&snapshot, width, height, duration_ms).then_some(snapshot)
    }

    pub(crate) fn for_frame(mut self, frame: RecordingFrameGeometry) -> Self {
        use super::{
            snapshot::RecordingTelemetryActionEvent, snapshot::signal::RecordingTelemetrySignal,
        };
        self.viewport = Some(
            serde_json::json!({"width": frame.width, "height": frame.height, "devicePixelRatio": 1, "scrollX": 0, "scrollY": 0}),
        );
        self.action_events.retain_mut(|event| match event {
            RecordingTelemetryActionEvent::Click { x, y, .. } => {
                frame.point(*x, *y).is_some_and(|point| {
                    (*x, *y) = point;
                    true
                })
            }
            RecordingTelemetryActionEvent::Key { .. } => true,
        });
        self.signals.retain_mut(|signal| match signal {
            RecordingTelemetrySignal::CursorIdle { x, y, .. } => {
                frame.point(*x, *y).is_some_and(|point| {
                    (*x, *y) = point;
                    true
                })
            }
            _ => true,
        });
        if let Some(samples) = self
            .cursor_track
            .as_mut()
            .and_then(|track| track["samples"].as_array_mut())
        {
            for sample in samples {
                let point = sample["x"]
                    .as_i64()
                    .and_then(|value| i32::try_from(value).ok())
                    .zip(
                        sample["y"]
                            .as_i64()
                            .and_then(|value| i32::try_from(value).ok()),
                    )
                    .and_then(|(x, y)| frame.point(x, y));
                let (x, y) = point.unwrap_or((0, 0));
                sample["x"] = serde_json::json!(x);
                sample["y"] = serde_json::json!(y);
                if point.is_none() {
                    sample["visible"] = serde_json::json!(false);
                }
            }
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telemetry::{
        RecordingTelemetryActionEvent, RecordingTelemetrySignal,
        SanitizedRecordingTelemetrySnapshot, snapshot::RecordingTelemetryCursorSample,
    };

    #[test]
    fn finalization_clips_optional_telemetry_to_the_encoded_media_and_reindexes_samples()
    -> Result<(), Box<dyn std::error::Error>> {
        let frame = RecordingFrameGeometry {
            x: -10,
            y: -20,
            width: 100,
            height: 80,
        };
        let snapshot = SanitizedRecordingTelemetrySnapshot::new(
            vec![
                RecordingTelemetryActionEvent::click("left", 11, -9, -19),
                RecordingTelemetryActionEvent::click("left", 10, -9, -19),
            ],
            vec![
                RecordingTelemetrySignal::typing_span(2, 5, 15),
                RecordingTelemetrySignal::cursor_idle(11, 15, -9, -19),
                RecordingTelemetrySignal::telemetry_warning("mouse-hook", "events-dropped", 10),
            ],
        )
        .with_cursor_samples(vec![
            RecordingTelemetryCursorSample::new(11, -9, -19, true),
            RecordingTelemetryCursorSample::new(10, -9, -19, true),
        ]);
        assert!(!crate::telemetry::valid_retained_telemetry(
            &snapshot.clone().for_frame(frame).into_protocol(),
            100,
            80,
            10,
        ));
        let projected = snapshot
            .for_finalized_media(Some(frame), 100, 80, 10)
            .ok_or("projection")?;
        assert_eq!(projected.action_events.len(), 1);
        assert_eq!(projected.action_events[0]["id"], "native-click-0");
        assert_eq!(projected.action_events[0]["point"]["x"], 1);
        assert_eq!(projected.signals.len(), 2);
        assert_eq!(projected.signals[0]["endTime"], 10);
        let track = projected.cursor_track.as_ref().ok_or("cursor")?;
        assert_eq!(track["samples"].as_array().ok_or("samples")?.len(), 1);
        assert_eq!(track["samples"][0]["id"], "native-cursor-0");
        assert!(crate::telemetry::valid_retained_telemetry(
            &projected, 100, 80, 10
        ));
        Ok(())
    }

    #[test]
    fn missing_or_mismatched_geometry_cannot_publish_unbounded_optional_telemetry() {
        let snapshot = SanitizedRecordingTelemetrySnapshot::new(vec![], vec![]);
        assert!(
            snapshot
                .clone()
                .for_finalized_media(None, 100, 80, 10)
                .is_none()
        );
        assert!(
            snapshot
                .for_finalized_media(
                    Some(RecordingFrameGeometry {
                        x: 0,
                        y: 0,
                        width: 200,
                        height: 80,
                    }),
                    100,
                    80,
                    10
                )
                .is_none()
        );
    }

    #[test]
    fn negative_monitor_region_and_window_points_stay_within_the_recorded_frame() {
        let frame = RecordingFrameGeometry {
            x: -1920,
            y: -100,
            width: 1280,
            height: 720,
        };
        assert_eq!(frame.point(-1900, -80), Some((20, 20)));
        assert_eq!(frame.point(-1921, -100), None);
        assert_eq!(frame.point(-640, 0), None);
        assert_eq!(frame.point(i32::MAX, 0), None);
    }
}
