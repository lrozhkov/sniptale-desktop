#[cfg(any(windows, test))]
mod cursor;
mod signal;

use crate::protocol::NativeRecordingTelemetrySnapshot;
use serde_json::Value;
#[cfg(any(windows, test))]
use serde_json::json;

use super::RecordingTelemetryPolicy;
#[cfg(any(windows, test))]
use super::sanitize::SanitizedKeySignal;
#[cfg(any(windows, test))]
pub(crate) use cursor::RecordingTelemetryCursorSample;
#[cfg(any(windows, test))]
use cursor::cursor_track_from_samples;
pub(crate) use signal::RecordingTelemetrySignal;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SanitizedRecordingTelemetrySnapshot {
    viewport: Option<Value>,
    cursor_track: Option<Value>,
    action_events: Vec<RecordingTelemetryActionEvent>,
    signals: Vec<RecordingTelemetrySignal>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum RecordingTelemetryActionEvent {
    #[cfg(any(windows, test))]
    Key { label: String, time_ms: u64 },
    #[cfg(any(windows, test))]
    Click {
        button: &'static str,
        time_ms: u64,
        x: i32,
        y: i32,
    },
}

pub(crate) trait RecordingTelemetrySource {
    fn bind_controller_lease(&mut self, _controller_lease_id: &str) {}

    fn configure_recording(&mut self, _policy: RecordingTelemetryPolicy) {}

    fn start_recording_with_policy(
        &mut self,
        policy: RecordingTelemetryPolicy,
        recording_id: &str,
        started_at_monotonic_ns: u64,
    ) {
        self.configure_recording(policy);
        self.start_recording(recording_id, started_at_monotonic_ns);
    }

    fn start_recording(&mut self, _recording_id: &str, _started_at_monotonic_ns: u64) {}

    fn pause_recording(&mut self, _recording_id: &str, _paused_at_monotonic_ns: u64) {}

    fn resume_recording(&mut self, _recording_id: &str, _resumed_at_monotonic_ns: u64) {}

    fn finish_recording(&mut self, _recording_id: &str, _stopped_at_monotonic_ns: u64) {}

    fn cancel_recording(&mut self, _recording_id: &str, _cancelled_at_monotonic_ns: u64) {}

    fn discard_recording(&mut self, _recording_id: &str) {}

    fn snapshot_for_recording(
        &mut self,
        recording_id: &str,
        duration_ms: u64,
    ) -> Option<SanitizedRecordingTelemetrySnapshot>;
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct NoopRecordingTelemetrySource;

impl RecordingTelemetrySource for NoopRecordingTelemetrySource {
    fn snapshot_for_recording(
        &mut self,
        _recording_id: &str,
        _duration_ms: u64,
    ) -> Option<SanitizedRecordingTelemetrySnapshot> {
        None
    }
}

impl SanitizedRecordingTelemetrySnapshot {
    #[cfg(any(windows, test))]
    pub(crate) fn new(
        action_events: Vec<RecordingTelemetryActionEvent>,
        signals: Vec<RecordingTelemetrySignal>,
    ) -> Self {
        Self {
            viewport: None,
            cursor_track: None,
            action_events,
            signals,
        }
    }

    #[cfg(any(windows, test))]
    pub(crate) fn with_cursor_samples(
        mut self,
        samples: Vec<RecordingTelemetryCursorSample>,
    ) -> Self {
        if !samples.is_empty() {
            self.cursor_track = Some(cursor_track_from_samples(samples));
        }
        self
    }

    #[cfg(test)]
    pub(crate) fn with_viewport(mut self, viewport: Value) -> Self {
        self.viewport = Some(viewport);
        self
    }

    #[cfg(test)]
    pub(crate) fn with_cursor_track(mut self, cursor_track: Value) -> Self {
        self.cursor_track = Some(cursor_track);
        self
    }

    pub(crate) fn into_protocol(self) -> NativeRecordingTelemetrySnapshot {
        NativeRecordingTelemetrySnapshot {
            viewport: self.viewport,
            cursor_track: self.cursor_track,
            action_events: self
                .action_events
                .into_iter()
                .enumerate()
                .map(|(index, event)| event.into_protocol(index))
                .collect(),
            signals: self
                .signals
                .into_iter()
                .enumerate()
                .map(|(index, signal)| signal.into_protocol(index))
                .collect(),
        }
    }
}

impl RecordingTelemetryActionEvent {
    #[cfg(any(windows, test))]
    pub(crate) fn from_key_signal(signal: SanitizedKeySignal, time_ms: u64) -> Option<Self> {
        let label = match signal {
            SanitizedKeySignal::KeyLabel(label) | SanitizedKeySignal::Shortcut(label) => label,
            SanitizedKeySignal::Typing { .. } | SanitizedKeySignal::Ignored => return None,
        };
        Some(Self::Key { label, time_ms })
    }

    #[cfg(any(windows, test))]
    pub(crate) const fn click(button: &'static str, time_ms: u64, x: i32, y: i32) -> Self {
        Self::Click {
            button,
            time_ms,
            x,
            y,
        }
    }

    fn into_protocol(self, index: usize) -> Value {
        #[cfg(not(any(windows, test)))]
        let _ = index;
        match self {
            #[cfg(any(windows, test))]
            Self::Key { label, time_ms } => json!({
                "id": format!("native-key-{index}"),
                "kind": "KEY",
                "label": label,
                "time": time_ms,
                "duration": 0,
                "point": null,
                "data": {},
                "preset": "NONE"
            }),
            #[cfg(any(windows, test))]
            Self::Click {
                button,
                time_ms,
                x,
                y,
            } => json!({
                "id": format!("native-click-{index}"),
                "kind": "CLICK",
                "label": "Click",
                "time": time_ms,
                "duration": 0,
                "point": {
                    "x": x,
                    "y": y
                },
                "data": {
                    "button": button
                },
                "preset": "NONE"
            }),
        }
    }
}
