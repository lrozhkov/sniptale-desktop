//! Runtime recording telemetry source wiring.

mod best_effort;
#[cfg(windows)]
mod drain;
#[cfg(windows)]
mod hooks;

use super::{RecordingTelemetryCollector, RecordingTelemetryPolicy, RecordingTelemetrySource};
#[cfg(any(test, windows))]
use super::{collector::TelemetryCollectorError, sanitize::SanitizedKeySignal};

#[cfg(test)]
use super::windows::WindowsKeyEvent;
#[cfg(any(test, windows))]
use super::windows::WindowsMouseEvent;
#[cfg(any(test, windows))]
use super::windows::WindowsMouseEventKind;
#[cfg(test)]
use super::windows::sanitize_windows_key_event;

#[cfg(windows)]
use super::windows::{WindowsKeyboardTelemetryHook, WindowsMouseTelemetryHook};

#[derive(Debug, Default)]
pub(crate) struct RuntimeRecordingTelemetrySource {
    collector: RecordingTelemetryCollector,
    recording_id: Option<String>,
    paused: bool,
    policy: RecordingTelemetryPolicy,
    #[cfg(windows)]
    keyboard_hook: Option<WindowsKeyboardTelemetryHook>,
    #[cfg(windows)]
    mouse_hook: Option<WindowsMouseTelemetryHook>,
}

#[cfg(windows)]
pub(crate) fn default_recording_telemetry_source() -> RuntimeRecordingTelemetrySource {
    RuntimeRecordingTelemetrySource::default()
}

impl RuntimeRecordingTelemetrySource {
    #[cfg(test)]
    pub(crate) fn inject_key_event(
        &mut self,
        recording_id: &str,
        event: WindowsKeyEvent,
        monotonic_ns: u64,
    ) -> Result<(), TelemetryCollectorError> {
        self.record_key_signal(
            recording_id,
            sanitize_windows_key_event(event),
            monotonic_ns,
        )
    }

    #[cfg(test)]
    pub(crate) fn inject_mouse_event(
        &mut self,
        recording_id: &str,
        event: WindowsMouseEvent,
        monotonic_ns: u64,
    ) -> Result<(), TelemetryCollectorError> {
        self.record_mouse_event(recording_id, event, monotonic_ns)
    }

    #[cfg(test)]
    pub(crate) fn inject_telemetry_warning(
        &mut self,
        recording_id: &str,
        source: &'static str,
        code: &'static str,
        monotonic_ns: u64,
    ) -> Result<(), TelemetryCollectorError> {
        self.collector
            .record_telemetry_warning(recording_id, source, code, monotonic_ns)
    }

    #[cfg(any(test, windows))]
    fn record_key_signal(
        &mut self,
        recording_id: &str,
        signal: SanitizedKeySignal,
        monotonic_ns: u64,
    ) -> Result<(), TelemetryCollectorError> {
        if self.recording_id.as_deref() != Some(recording_id) {
            return Err(TelemetryCollectorError::NotRecording);
        }
        let enabled = match signal {
            SanitizedKeySignal::Typing { .. } => self.policy.collect_typing_spans,
            SanitizedKeySignal::KeyLabel(_) | SanitizedKeySignal::Shortcut(_) => {
                self.policy.collect_key_events
            }
            SanitizedKeySignal::Ignored => false,
        };
        if enabled {
            self.collector
                .record_signal(recording_id, signal, monotonic_ns)?;
        }
        Ok(())
    }

    #[cfg(any(test, windows))]
    fn record_mouse_event(
        &mut self,
        recording_id: &str,
        event: WindowsMouseEvent,
        monotonic_ns: u64,
    ) -> Result<(), TelemetryCollectorError> {
        if self.recording_id.as_deref() != Some(recording_id) {
            return Err(TelemetryCollectorError::NotRecording);
        }
        match event.kind {
            WindowsMouseEventKind::Move if self.policy.collect_cursor => {
                self.collector
                    .record_cursor(recording_id, event.x, event.y, monotonic_ns)?;
            }
            WindowsMouseEventKind::Click(button) if self.policy.collect_clicks => {
                self.collector.record_click(
                    recording_id,
                    button.label(),
                    event.x,
                    event.y,
                    monotonic_ns,
                )?;
                if self.policy.collect_cursor {
                    self.collector
                        .record_cursor(recording_id, event.x, event.y, monotonic_ns)?;
                }
            }
            WindowsMouseEventKind::Move | WindowsMouseEventKind::Click(_) => {}
        }
        Ok(())
    }

    #[cfg(not(windows))]
    fn drain_runtime_events(&mut self, monotonic_ns: u64, record: bool) {
        let _ = (self.recording_id.as_deref(), monotonic_ns, record);
    }

    #[cfg(windows)]
    fn clear_runtime_hooks(&mut self) {
        self.keyboard_hook = None;
        self.mouse_hook = None;
    }
}

impl RecordingTelemetrySource for RuntimeRecordingTelemetrySource {
    fn configure_recording(&mut self, policy: RecordingTelemetryPolicy) {
        if self.recording_id.is_none() {
            self.policy = policy;
        }
    }

    fn start_recording(&mut self, recording_id: &str, started_at_monotonic_ns: u64) {
        self.recording_id = Some(recording_id.to_owned());
        self.paused = false;
        self.collector
            .start_recording(recording_id, started_at_monotonic_ns);
        #[cfg(windows)]
        {
            self.install_runtime_hooks(recording_id, started_at_monotonic_ns);
        }
    }

    fn pause_recording(&mut self, recording_id: &str, paused_at_monotonic_ns: u64) {
        self.drain_runtime_events(paused_at_monotonic_ns, true);
        self.collector
            .pause_recording(recording_id, paused_at_monotonic_ns);
        self.paused = true;
    }

    fn resume_recording(&mut self, recording_id: &str, resumed_at_monotonic_ns: u64) {
        self.drain_runtime_events(resumed_at_monotonic_ns, false);
        self.collector
            .resume_recording(recording_id, resumed_at_monotonic_ns);
        self.paused = false;
    }

    fn finish_recording(&mut self, recording_id: &str, stopped_at_monotonic_ns: u64) {
        self.drain_runtime_events(stopped_at_monotonic_ns, !self.paused);
        if self.policy.collect_static_signals
            && self
                .collector
                .record_cursor_idle_signal(recording_id, stopped_at_monotonic_ns)
                .is_err()
        {
            self.record_telemetry_warning_best_effort(
                recording_id,
                "cursor-telemetry",
                "idle-signal-record-failed",
                stopped_at_monotonic_ns,
            );
        }
        self.collector
            .finish_recording(recording_id, stopped_at_monotonic_ns);
        clear_runtime_hooks(self);
    }

    fn cancel_recording(&mut self, recording_id: &str, cancelled_at_monotonic_ns: u64) {
        self.drain_runtime_events(cancelled_at_monotonic_ns, false);
        self.collector
            .cancel_recording(recording_id, cancelled_at_monotonic_ns);
        clear_runtime_hooks(self);
        self.recording_id = None;
        self.paused = false;
    }

    fn discard_recording(&mut self, recording_id: &str) {
        self.collector.discard_recording(recording_id);
        clear_runtime_hooks(self);
        self.recording_id = None;
        self.paused = false;
    }

    fn snapshot_for_recording(
        &mut self,
        recording_id: &str,
        duration_ms: u64,
    ) -> Option<super::SanitizedRecordingTelemetrySnapshot> {
        self.recording_id = None;
        self.paused = false;
        self.collector
            .snapshot_for_recording(recording_id, duration_ms)
    }
}

fn clear_runtime_hooks(source: &mut RuntimeRecordingTelemetrySource) {
    #[cfg(windows)]
    source.clear_runtime_hooks();
    #[cfg(not(windows))]
    let _ = source;
}
