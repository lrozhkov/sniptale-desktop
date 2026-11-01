//! Join bounded Windows hook prefixes before publishing lifecycle or loss barriers.

#[cfg(windows)]
use super::RuntimeRecordingTelemetrySource;
use crate::telemetry::windows::{WindowsKeyObservation, WindowsMouseEvent};

#[cfg(windows)]
impl RuntimeRecordingTelemetrySource {
    pub(super) fn drain_runtime_events(&mut self, monotonic_ns: u64, record: bool) {
        if self.keyboard_hook.is_some() || self.mouse_hook.is_some() {
            crate::telemetry::windows::pump_current_thread_hook_messages();
        }
        let Some(recording_id) = self.recording_id.clone() else {
            return;
        };
        let cutoff = self
            .source_clock_mapping
            .and_then(|clock| clock.source_cutoff_ms(monotonic_ns).ok());
        let keyboard = self
            .keyboard_hook
            .as_mut()
            .map(|hook| hook.drain_events(cutoff));
        let mouse = self
            .mouse_hook
            .as_mut()
            .map(|hook| hook.drain_events(cutoff));
        let mut warnings = Vec::new();
        let keyboard = match keyboard {
            Some(Ok(snapshot)) => {
                if snapshot.dropped_events > 0 {
                    warnings.push(("keyboard-hook", "events-dropped"));
                }
                snapshot.events
            }
            Some(Err(_)) => {
                warnings.push(("keyboard-hook", "drain-failed"));
                Vec::new()
            }
            None => Vec::new(),
        };
        let mut last_mouse = None;
        let mouse = match mouse {
            Some(Ok(snapshot)) => {
                last_mouse = snapshot.last_observation_seq;
                if snapshot.dropped_events > 0 {
                    warnings.push(("mouse-hook", "events-dropped"));
                }
                snapshot.events
            }
            Some(Err(_)) => {
                if let Some(cursor) = self.cursor_trajectory.as_mut() {
                    cursor.fail();
                }
                warnings.push(("mouse-hook", "drain-failed"));
                Vec::new()
            }
            None => Vec::new(),
        };
        for observation in merge_input_observations(keyboard, mouse) {
            if record {
                if let Some(warning) =
                    self.record_joined_observation(&recording_id, observation, monotonic_ns)
                {
                    warnings.push(warning);
                }
            } else if let Some(history) = self.event_history.as_mut() {
                match observation {
                    WindowsInputObservation::Keyboard(event) => {
                        history.paused_keyboard_baseline(event.sequence);
                    }
                    WindowsInputObservation::Mouse(event) => {
                        history.paused_mouse_baseline(event.sequence);
                    }
                }
            }
        }
        if let Some(cursor) = self.cursor_trajectory.as_mut()
            && cursor.tail_loss(last_mouse).is_err()
        {
            cursor.fail();
        }
        // Loss warnings close a shared event segment only after both accepted prefixes are joined.
        for (source, code) in warnings {
            self.record_telemetry_warning_best_effort(&recording_id, source, code, monotonic_ns);
        }
    }

    fn record_joined_observation(
        &mut self,
        recording_id: &str,
        observation: WindowsInputObservation,
        monotonic_ns: u64,
    ) -> Option<(&'static str, &'static str)> {
        match observation {
            WindowsInputObservation::Keyboard(event) => self
                .record_key_observation(recording_id, event)
                .err()
                .map(|_| ("keyboard-hook", "event-record-failed")),
            WindowsInputObservation::Mouse(event) => self
                .record_mouse_event(recording_id, event, monotonic_ns)
                .err()
                .map(|error| {
                    if error
                        == crate::telemetry::collector::TelemetryCollectorError::CursorTrajectory
                    {
                        ("cursor-trajectory", "format-unavailable")
                    } else {
                        ("mouse-hook", "event-record-failed")
                    }
                }),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WindowsInputObservation {
    Keyboard(WindowsKeyObservation),
    Mouse(WindowsMouseEvent),
}

/// Merge bounded queue prefixes by original source time without reordering either producer.
pub(crate) fn merge_input_observations(
    keyboard: Vec<WindowsKeyObservation>,
    mouse: Vec<WindowsMouseEvent>,
) -> impl Iterator<Item = WindowsInputObservation> {
    let mut keyboard = keyboard.into_iter().peekable();
    let mut mouse = mouse.into_iter().peekable();
    std::iter::from_fn(move || match (keyboard.peek(), mouse.peek()) {
        (Some(key), Some(pointer)) if key.source_time_ms <= pointer.source_time_ms => {
            keyboard.next().map(WindowsInputObservation::Keyboard)
        }
        (Some(_) | None, Some(_)) => mouse.next().map(WindowsInputObservation::Mouse),
        (Some(_), None) => keyboard.next().map(WindowsInputObservation::Keyboard),
        (None, None) => None,
    })
}
