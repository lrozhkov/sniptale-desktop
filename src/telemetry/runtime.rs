//! Runtime recording telemetry source wiring.

#[cfg(windows)]
mod best_effort;
mod cursor;
mod drain;
#[cfg(windows)]
mod hooks;

use super::{RecordingTelemetryCollector, RecordingTelemetryPolicy, RecordingTelemetrySource};
#[cfg(any(test, windows))]
use super::{collector::TelemetryCollectorError, sanitize::SanitizedKeySignal};

#[cfg(any(test, windows))]
pub(crate) use super::windows::WindowsMouseEvent;
#[cfg(any(test, windows))]
pub(crate) use super::windows::WindowsMouseEventKind;
#[cfg(test)]
use super::windows::sanitize_windows_key_event;
#[cfg(test)]
pub(crate) use super::windows::{
    WindowsKeyEvent, WindowsKeyModifiers, WindowsKeyObservation, WindowsKeyPhase,
    WindowsMouseButton,
};
#[cfg(test)]
pub(crate) use drain::{WindowsInputObservation, merge_input_observations};

#[cfg(windows)]
use super::windows::{WindowsKeyboardTelemetryHook, WindowsMouseTelemetryHook};

#[derive(Debug, Default)]
pub(crate) struct RuntimeRecordingTelemetrySource {
    collector: RecordingTelemetryCollector,
    recording_id: Option<String>,
    paused: bool,
    policy: RecordingTelemetryPolicy,
    trajectory_context: Option<super::RecordingTrajectoryContext>,
    cursor_trajectory: Option<cursor::RuntimeCursorTrajectory>,
    trajectory_factory: Option<Box<dyn super::CursorTrajectorySinkFactory>>,
    recording_clock: Option<fn() -> u64>,
    source_clock: Option<fn() -> u64>,
    source_clock_mapping: Option<super::trajectory_clock::WindowsTrajectoryClock>,
    events_factory: Option<Box<dyn super::RecordingEventsSinkFactory>>,
    event_history: Option<super::events::NativeRecordingEvents>,
    #[cfg(windows)]
    keyboard_hook: Option<WindowsKeyboardTelemetryHook>,
    #[cfg(windows)]
    mouse_hook: Option<WindowsMouseTelemetryHook>,
    #[cfg(windows)]
    hooks_enabled: bool,
}

#[cfg(windows)]
pub(crate) fn default_recording_telemetry_source(
    recording_clock: fn() -> u64,
    factory: Option<Box<dyn super::CursorTrajectorySinkFactory>>,
    events_factory: Option<Box<dyn super::RecordingEventsSinkFactory>>,
) -> RuntimeRecordingTelemetrySource {
    let mut source = RuntimeRecordingTelemetrySource::with_trajectory_factory(
        recording_clock,
        super::windows::current_boot_ms,
        factory,
    );
    source.hooks_enabled = true;
    source.events_factory = events_factory;
    source
}

impl RuntimeRecordingTelemetrySource {
    pub(crate) fn with_trajectory_factory(
        recording_clock: fn() -> u64,
        source_clock: fn() -> u64,
        trajectory_factory: Option<Box<dyn super::CursorTrajectorySinkFactory>>,
    ) -> Self {
        Self {
            collector: RecordingTelemetryCollector::default(),
            recording_id: None,
            paused: false,
            policy: RecordingTelemetryPolicy::default(),
            trajectory_context: None,
            cursor_trajectory: None,
            recording_clock: Some(recording_clock),
            source_clock: Some(source_clock),
            source_clock_mapping: None,
            events_factory: None,
            event_history: None,
            trajectory_factory,
            #[cfg(windows)]
            keyboard_hook: None,
            #[cfg(windows)]
            mouse_hook: None,
            #[cfg(windows)]
            hooks_enabled: false,
        }
    }

    #[cfg(test)]
    pub(crate) fn with_events_factory(
        mut self,
        factory: Box<dyn super::RecordingEventsSinkFactory>,
    ) -> Self {
        self.events_factory = Some(factory);
        self
    }

    #[cfg(test)]
    pub(crate) fn inject_key_observation(
        &mut self,
        recording: &str,
        observation: super::windows::WindowsKeyObservation,
    ) -> Result<(), TelemetryCollectorError> {
        self.record_key_observation(recording, observation)
    }

    fn erase_histories(&mut self) {
        if self
            .cursor_trajectory
            .as_mut()
            .is_some_and(|cursor| cursor.erase().is_err())
        {
            tracing::warn!(
                source = "cursor-trajectory",
                "recording history cleanup remains pending"
            );
        }
        self.cursor_trajectory = None;
        if self
            .event_history
            .as_mut()
            .is_some_and(|history| history.erase().is_err())
        {
            tracing::warn!(
                source = "recording-events",
                "recording history cleanup remains pending"
            );
        }
        self.event_history = None;
    }

    fn start_cursor_trajectory(&mut self, recording_id: &str, started_ns: u64) {
        if !self.policy.collect_cursor {
            return;
        }
        let Some(context) = self.trajectory_context.clone() else {
            return;
        };
        let Some(trajectory_id) = random_history_id() else {
            return;
        };
        let clock = self.source_clock_mapping;
        let factory = self
            .trajectory_factory
            .as_mut()
            .map(|factory| factory.as_mut() as &mut dyn super::CursorTrajectorySinkFactory);
        self.cursor_trajectory = Some(cursor::RuntimeCursorTrajectory::new(
            recording_id,
            trajectory_id,
            context,
            started_ns,
            clock,
            factory,
        ));
    }
    fn start_event_history(&mut self, recording: &str, started: u64) {
        if self.events_factory.is_none() {
            return;
        }
        if !(self.policy.collect_key_events
            || self.policy.collect_typing_spans
            || self.policy.collect_clicks
            || self.policy.collect_static_signals)
        {
            return;
        }
        let (Some(context), Some(source)) = (self.trajectory_context.as_ref(), random_history_id())
        else {
            return;
        };
        let factory = self
            .events_factory
            .as_mut()
            .map(|factory| factory.as_mut() as &mut dyn super::RecordingEventsSinkFactory);
        self.event_history = Some(super::events::NativeRecordingEvents::new(
            recording,
            source,
            context,
            self.policy,
            started,
            self.source_clock_mapping,
            factory,
        ));
    }

    #[cfg(any(test, windows))]
    fn record_key_observation(
        &mut self,
        recording: &str,
        observation: super::windows::WindowsKeyObservation,
    ) -> Result<(), TelemetryCollectorError> {
        if self.recording_id.as_deref() != Some(recording) {
            return Err(TelemetryCollectorError::RecordingMismatch);
        }
        if let Some(history) = self.event_history.as_mut() {
            return history.keyboard(observation).map_err(|_| {
                history.fail();
                TelemetryCollectorError::RecordingEvents
            });
        }
        if observation.phase == super::windows::WindowsKeyPhase::Up {
            return Ok(());
        }
        let time = self
            .source_clock_mapping
            .ok_or(TelemetryCollectorError::RecordingEvents)?
            .event_monotonic_ns(observation.source_time_ms)
            .map_err(|_| TelemetryCollectorError::RecordingEvents)?;
        self.record_key_signal(
            recording,
            super::windows::sanitize_windows_key_event(observation.event),
            time,
        )
    }

    fn record_warning(
        &mut self,
        recording: &str,
        source: &'static str,
        code: &'static str,
        time: u64,
    ) -> Result<(), TelemetryCollectorError> {
        if self.recording_id.as_deref() != Some(recording) {
            return Err(TelemetryCollectorError::RecordingMismatch);
        }
        if let Some(history) = self.event_history.as_mut() {
            return history.warning(source, code, time).map_err(|_| {
                history.fail();
                TelemetryCollectorError::RecordingEvents
            });
        }
        self.collector
            .record_telemetry_warning(recording, source, code, time)
    }

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
        self.record_warning(recording_id, source, code, monotonic_ns)
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
        let _ = monotonic_ns;
        let mut cursor_failed = false;
        if let Some(cursor) = self.cursor_trajectory.as_mut() {
            use crate::protocol::trajectory::{CursorTrajectoryButton, CursorTrajectoryEvent};
            let (kind, button) = match event.kind {
                WindowsMouseEventKind::Move => {
                    (CursorTrajectoryEvent::Move, CursorTrajectoryButton::None)
                }
                WindowsMouseEventKind::Down(button) => (CursorTrajectoryEvent::Down, button.into()),
                WindowsMouseEventKind::Up(button) => (CursorTrajectoryEvent::Up, button.into()),
            };
            let (kind, button) = if self.policy.collect_clicks {
                (kind, button)
            } else {
                (CursorTrajectoryEvent::Move, CursorTrajectoryButton::None)
            };
            if cursor
                .observe(cursor::CursorSourceObservation {
                    sequence: event.sequence,
                    source_time_ms: event.source_time_ms,
                    visible: event.visible,
                    x: event.x,
                    y: event.y,
                    event: kind,
                    button,
                })
                .is_err()
            {
                cursor.fail();
                cursor_failed = true;
            }
        }
        if let Some(history) = self.event_history.as_mut()
            && history.mouse(event).is_err()
        {
            history.fail();
            return Err(TelemetryCollectorError::RecordingEvents);
        }
        if cursor_failed {
            Err(TelemetryCollectorError::CursorTrajectory)
        } else {
            Ok(())
        }
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
    fn configure_trajectory(&mut self, context: Option<super::RecordingTrajectoryContext>) {
        if self.recording_id.is_none() {
            self.trajectory_context = context;
        }
    }
    fn poll_recording_events(&mut self, observed_monotonic_ns: u64) {
        if self.recording_id.is_some() {
            self.drain_runtime_events(observed_monotonic_ns, !self.paused);
        }
    }
    fn configure_recording(&mut self, policy: RecordingTelemetryPolicy) {
        if self.recording_id.is_none() {
            self.policy = policy;
        }
    }

    fn start_recording(&mut self, recording_id: &str, started_at_monotonic_ns: u64) {
        if self.recording_id.is_some() {
            return;
        }
        self.recording_id = Some(recording_id.to_owned());
        self.paused = false;
        self.source_clock_mapping =
            self.recording_clock
                .zip(self.source_clock)
                .and_then(|(recording, source)| {
                    let before = recording();
                    let boot = source();
                    super::trajectory_clock::WindowsTrajectoryClock::calibrate(
                        boot,
                        before,
                        recording(),
                    )
                    .ok()
                });
        self.collector
            .start_recording(recording_id, started_at_monotonic_ns);
        self.start_cursor_trajectory(recording_id, started_at_monotonic_ns);
        self.start_event_history(recording_id, started_at_monotonic_ns);
        if self.cursor_trajectory.as_ref().is_some_and(|cursor| {
            cursor.descriptor().state
                == crate::protocol::trajectory::CursorTrajectoryState::Unavailable
        }) {
            let _ = self.record_warning(
                recording_id,
                "cursor-trajectory",
                "format-unavailable",
                started_at_monotonic_ns,
            );
        }
        #[cfg(windows)]
        {
            if self.hooks_enabled {
                self.install_runtime_hooks(recording_id, started_at_monotonic_ns);
            }
        }
    }

    fn pause_recording(&mut self, recording_id: &str, paused_at_monotonic_ns: u64) {
        if self.recording_id.as_deref() != Some(recording_id) {
            return;
        }
        self.drain_runtime_events(paused_at_monotonic_ns, true);
        if let Some(history) = self.event_history.as_mut()
            && history.pause(paused_at_monotonic_ns).is_err()
        {
            history.fail();
        }
        if let Some(cursor) = self.cursor_trajectory.as_mut()
            && cursor.pause(paused_at_monotonic_ns).is_err()
        {
            cursor.fail();
        }
        self.collector
            .pause_recording(recording_id, paused_at_monotonic_ns);
        self.paused = true;
    }

    fn resume_recording(&mut self, recording_id: &str, resumed_at_monotonic_ns: u64) {
        if self.recording_id.as_deref() != Some(recording_id) {
            return;
        }
        self.drain_runtime_events(resumed_at_monotonic_ns, false);
        if let Some(history) = self.event_history.as_mut()
            && history.resume(resumed_at_monotonic_ns).is_err()
        {
            history.fail();
        }
        if let Some(cursor) = self.cursor_trajectory.as_mut()
            && cursor.resume(resumed_at_monotonic_ns).is_err()
        {
            cursor.fail();
        }
        self.collector
            .resume_recording(recording_id, resumed_at_monotonic_ns);
        self.paused = false;
    }

    fn finish_recording(&mut self, recording_id: &str, stopped_at_monotonic_ns: u64) {
        if self.recording_id.as_deref() != Some(recording_id) {
            return;
        }
        self.drain_runtime_events(stopped_at_monotonic_ns, !self.paused);
        clear_runtime_hooks(self);
        if let Some(history) = self.event_history.as_mut() {
            history.finish(stopped_at_monotonic_ns);
        }
        if let Some(cursor) = self.cursor_trajectory.as_mut() {
            cursor.finish(stopped_at_monotonic_ns);
        }
        self.collector
            .finish_recording(recording_id, stopped_at_monotonic_ns);
        clear_runtime_hooks(self);
    }

    fn cancel_recording(&mut self, recording_id: &str, cancelled_at_monotonic_ns: u64) {
        if self.recording_id.as_deref() != Some(recording_id) {
            return;
        }
        self.drain_runtime_events(cancelled_at_monotonic_ns, false);
        clear_runtime_hooks(self);
        self.erase_histories();
        self.collector
            .cancel_recording(recording_id, cancelled_at_monotonic_ns);
        clear_runtime_hooks(self);
        self.recording_id = None;
        self.paused = false;
    }

    fn discard_recording(&mut self, recording_id: &str) {
        if self.recording_id.as_deref() != Some(recording_id) {
            return;
        }
        clear_runtime_hooks(self);
        self.erase_histories();
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
        if self.recording_id.as_deref() != Some(recording_id) {
            return None;
        }
        let descriptor = self
            .cursor_trajectory
            .take()
            .map(|cursor| cursor.descriptor().clone());
        let events = self
            .event_history
            .take()
            .map(|history| history.descriptor().clone());
        self.recording_id = None;
        self.paused = false;
        self.collector
            .snapshot_for_recording(recording_id, duration_ms)
            .map(|mut snapshot| {
                snapshot.cursor_trajectory = descriptor;
                snapshot.recording_events = events;
                snapshot
            })
    }
}

fn clear_runtime_hooks(source: &mut RuntimeRecordingTelemetrySource) {
    #[cfg(windows)]
    source.clear_runtime_hooks();
    #[cfg(not(windows))]
    let _ = source;
}

impl Drop for RuntimeRecordingTelemetrySource {
    fn drop(&mut self) {
        clear_runtime_hooks(self);
    }
}

fn random_history_id() -> Option<String> {
    let mut bytes = [0; 16];
    getrandom::fill(&mut bytes).ok()?;
    let mut id = String::with_capacity(32);
    for byte in bytes {
        id.push(char::from(b"0123456789abcdef"[usize::from(byte >> 4)]));
        id.push(char::from(b"0123456789abcdef"[usize::from(byte & 15)]));
    }
    Some(id)
}
