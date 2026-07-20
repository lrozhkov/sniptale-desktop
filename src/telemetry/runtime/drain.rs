//! Windows telemetry hook event draining.
#![cfg(windows)]

use super::RuntimeRecordingTelemetrySource;
use crate::telemetry::{
    sanitize::SanitizedKeySignal,
    windows::{WindowsMouseEvent, sanitize_windows_key_event},
};

impl RuntimeRecordingTelemetrySource {
    pub(super) fn drain_runtime_events(&mut self, monotonic_ns: u64, record: bool) {
        self.drain_keyboard_events(monotonic_ns, record);
        self.drain_mouse_events(monotonic_ns, record);
    }

    fn drain_keyboard_events(&mut self, monotonic_ns: u64, record: bool) {
        let Some(recording_id) = self.recording_id.clone() else {
            return;
        };
        if let Some(hook) = self.keyboard_hook.as_mut() {
            match hook.drain_events() {
                Ok(snapshot) => {
                    if record {
                        for event in snapshot.events {
                            self.record_key_signal_best_effort(
                                &recording_id,
                                sanitize_windows_key_event(event),
                                monotonic_ns,
                            );
                        }
                    }
                    if snapshot.dropped_events > 0 {
                        self.record_telemetry_warning_best_effort(
                            &recording_id,
                            "keyboard-hook",
                            "events-dropped",
                            monotonic_ns,
                        );
                    }
                }
                Err(_) => self.record_telemetry_warning_best_effort(
                    &recording_id,
                    "keyboard-hook",
                    "drain-failed",
                    monotonic_ns,
                ),
            }
        }
    }

    fn drain_mouse_events(&mut self, monotonic_ns: u64, record: bool) {
        let Some(recording_id) = self.recording_id.clone() else {
            return;
        };
        if let Some(hook) = self.mouse_hook.as_mut() {
            match hook.drain_events() {
                Ok(snapshot) => {
                    if record {
                        for event in snapshot.events {
                            self.record_mouse_event_best_effort(&recording_id, event, monotonic_ns);
                        }
                    }
                    if snapshot.dropped_events > 0 {
                        self.record_telemetry_warning_best_effort(
                            &recording_id,
                            "mouse-hook",
                            "events-dropped",
                            monotonic_ns,
                        );
                    }
                }
                Err(_) => self.record_telemetry_warning_best_effort(
                    &recording_id,
                    "mouse-hook",
                    "drain-failed",
                    monotonic_ns,
                ),
            }
        }
    }

    fn record_key_signal_best_effort(
        &mut self,
        recording_id: &str,
        signal: SanitizedKeySignal,
        monotonic_ns: u64,
    ) {
        if self
            .record_key_signal(recording_id, signal, monotonic_ns)
            .is_err()
        {
            self.record_telemetry_warning_best_effort(
                recording_id,
                "keyboard-hook",
                "signal-record-failed",
                monotonic_ns,
            );
        }
    }

    fn record_mouse_event_best_effort(
        &mut self,
        recording_id: &str,
        event: WindowsMouseEvent,
        monotonic_ns: u64,
    ) {
        if self
            .record_mouse_event(recording_id, event, monotonic_ns)
            .is_err()
        {
            self.record_telemetry_warning_best_effort(
                recording_id,
                "mouse-hook",
                "event-record-failed",
                monotonic_ns,
            );
        }
    }
}
