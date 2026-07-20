//! Recording telemetry event collector.

mod typing;

#[cfg(test)]
use super::sanitize::{KeyCode, KeyModifiers, sanitize_key};
use super::{
    sanitize::SanitizedKeySignal,
    snapshot::{
        RecordingTelemetryActionEvent, RecordingTelemetryCursorSample, RecordingTelemetrySignal,
        SanitizedRecordingTelemetrySnapshot,
    },
    timebase::{MediaTimebase, PausedInterval, TimebaseError},
};
use typing::TypingSpanAccumulator;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TelemetryCollectorError {
    RecordingMismatch,
    NotRecording,
    Timebase(TimebaseError),
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct RecordingTelemetryCollector {
    session: Option<RecordingTelemetryCollectionSession>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RecordingTelemetryCollectionSession {
    recording_id: String,
    timebase: MediaTimebase,
    pauses: Vec<PausedInterval>,
    open_pause_started_monotonic_ns: Option<u64>,
    action_events: Vec<RecordingTelemetryActionEvent>,
    cursor_samples: Vec<RecordingTelemetryCursorSample>,
    signals: Vec<RecordingTelemetrySignal>,
    typing: TypingSpanAccumulator,
}

impl RecordingTelemetryCollector {
    pub(crate) fn start_recording(&mut self, recording_id: &str, started_at_monotonic_ns: u64) {
        self.session = Some(RecordingTelemetryCollectionSession {
            recording_id: recording_id.to_owned(),
            timebase: MediaTimebase::new(started_at_monotonic_ns),
            pauses: Vec::new(),
            open_pause_started_monotonic_ns: None,
            action_events: Vec::new(),
            cursor_samples: Vec::new(),
            signals: Vec::new(),
            typing: TypingSpanAccumulator::default(),
        });
    }

    #[cfg(test)]
    pub(crate) fn record_key(
        &mut self,
        recording_id: &str,
        key: KeyCode,
        modifiers: KeyModifiers,
        monotonic_ns: u64,
    ) -> Result<(), TelemetryCollectorError> {
        let signal = sanitize_key(key, modifiers);
        self.record_signal(recording_id, signal, monotonic_ns)
    }

    pub(crate) fn record_signal(
        &mut self,
        recording_id: &str,
        signal: SanitizedKeySignal,
        monotonic_ns: u64,
    ) -> Result<(), TelemetryCollectorError> {
        let session = self.session_for(recording_id)?;
        let time_ms = session.media_time_ms(monotonic_ns)?;
        match signal {
            SanitizedKeySignal::Typing { count } => session.typing.record(time_ms, count),
            SanitizedKeySignal::KeyLabel(_) | SanitizedKeySignal::Shortcut(_) => {
                session.flush_typing();
                if let Some(action) =
                    RecordingTelemetryActionEvent::from_key_signal(signal, time_ms)
                {
                    session.action_events.push(action);
                }
            }
            SanitizedKeySignal::Ignored => {}
        }
        Ok(())
    }

    pub(crate) fn record_cursor(
        &mut self,
        recording_id: &str,
        x: i32,
        y: i32,
        monotonic_ns: u64,
    ) -> Result<(), TelemetryCollectorError> {
        let session = self.session_for(recording_id)?;
        let time_ms = session.media_time_ms(monotonic_ns)?;
        session
            .cursor_samples
            .push(RecordingTelemetryCursorSample::new(time_ms, x, y, true));
        Ok(())
    }

    pub(crate) fn record_click(
        &mut self,
        recording_id: &str,
        button: &'static str,
        x: i32,
        y: i32,
        monotonic_ns: u64,
    ) -> Result<(), TelemetryCollectorError> {
        let session = self.session_for(recording_id)?;
        let time_ms = session.media_time_ms(monotonic_ns)?;
        session.flush_typing();
        session
            .action_events
            .push(RecordingTelemetryActionEvent::click(button, time_ms, x, y));
        Ok(())
    }

    pub(crate) fn record_telemetry_warning(
        &mut self,
        recording_id: &str,
        source: &'static str,
        code: &'static str,
        monotonic_ns: u64,
    ) -> Result<(), TelemetryCollectorError> {
        let session = self.session_for(recording_id)?;
        let time_ms = session.media_time_ms(monotonic_ns).unwrap_or(0);
        session
            .signals
            .push(RecordingTelemetrySignal::telemetry_warning(
                source, code, time_ms,
            ));
        Ok(())
    }

    pub(crate) fn record_cursor_idle_signal(
        &mut self,
        recording_id: &str,
        monotonic_ns: u64,
    ) -> Result<(), TelemetryCollectorError> {
        let session = self.session_for(recording_id)?;
        let Some(sample) = session.cursor_samples.last() else {
            return Ok(());
        };
        let end_time_ms = session.media_time_ms(monotonic_ns)?;
        session.signals.push(RecordingTelemetrySignal::cursor_idle(
            sample.time_ms,
            end_time_ms,
            sample.x,
            sample.y,
        ));
        Ok(())
    }

    pub(crate) fn record_pause(
        &mut self,
        recording_id: &str,
        start_monotonic_ns: u64,
        end_monotonic_ns: u64,
    ) -> Result<(), TelemetryCollectorError> {
        let session = self.session_for(recording_id)?;
        if end_monotonic_ns < start_monotonic_ns {
            return Err(TelemetryCollectorError::Timebase(
                TimebaseError::InvalidPause,
            ));
        }
        session.flush_typing();
        session.pauses.push(PausedInterval {
            start_monotonic_ns,
            end_monotonic_ns,
        });
        Ok(())
    }

    pub(crate) fn record_pause_start(
        &mut self,
        recording_id: &str,
        start_monotonic_ns: u64,
    ) -> Result<(), TelemetryCollectorError> {
        let session = self.session_for(recording_id)?;
        session.flush_typing();
        session.open_pause_started_monotonic_ns = Some(start_monotonic_ns);
        Ok(())
    }

    pub(crate) fn record_pause_end(
        &mut self,
        recording_id: &str,
        end_monotonic_ns: u64,
    ) -> Result<(), TelemetryCollectorError> {
        let session = self.session_for(recording_id)?;
        let Some(start_monotonic_ns) = session.open_pause_started_monotonic_ns.take() else {
            return Ok(());
        };
        self.record_pause(recording_id, start_monotonic_ns, end_monotonic_ns)
    }

    fn session_for(
        &mut self,
        recording_id: &str,
    ) -> Result<&mut RecordingTelemetryCollectionSession, TelemetryCollectorError> {
        let session = self
            .session
            .as_mut()
            .ok_or(TelemetryCollectorError::NotRecording)?;
        if session.recording_id == recording_id {
            Ok(session)
        } else {
            Err(TelemetryCollectorError::RecordingMismatch)
        }
    }
}

impl super::snapshot::RecordingTelemetrySource for RecordingTelemetryCollector {
    fn start_recording(&mut self, recording_id: &str, started_at_monotonic_ns: u64) {
        RecordingTelemetryCollector::start_recording(self, recording_id, started_at_monotonic_ns);
    }

    fn pause_recording(&mut self, recording_id: &str, paused_at_monotonic_ns: u64) {
        self.record_pause_start(recording_id, paused_at_monotonic_ns)
            .unwrap_or(());
    }

    fn resume_recording(&mut self, recording_id: &str, resumed_at_monotonic_ns: u64) {
        self.record_pause_end(recording_id, resumed_at_monotonic_ns)
            .unwrap_or(());
    }

    fn finish_recording(&mut self, recording_id: &str, stopped_at_monotonic_ns: u64) {
        self.record_pause_end(recording_id, stopped_at_monotonic_ns)
            .unwrap_or(());
    }

    fn cancel_recording(&mut self, recording_id: &str, _cancelled_at_monotonic_ns: u64) {
        self.discard_recording(recording_id);
    }

    fn discard_recording(&mut self, recording_id: &str) {
        if self
            .session
            .as_ref()
            .is_some_and(|session| session.recording_id == recording_id)
        {
            self.session = None;
        }
    }

    fn snapshot_for_recording(
        &mut self,
        recording_id: &str,
        duration_ms: u64,
    ) -> Option<SanitizedRecordingTelemetrySnapshot> {
        let mut session = self.session.take()?;
        if session.recording_id != recording_id {
            self.session = Some(session);
            return None;
        }
        session.typing.flush_at(duration_ms);
        session.flush_typing();
        Some(SanitizedRecordingTelemetrySnapshot::new(
            session.action_events,
            session.signals,
        ))
        .map(|snapshot| snapshot.with_cursor_samples(session.cursor_samples))
    }
}

impl RecordingTelemetryCollectionSession {
    fn media_time_ms(&self, monotonic_ns: u64) -> Result<u64, TelemetryCollectorError> {
        self.timebase
            .elapsed_media_ms(monotonic_ns, &self.pauses)
            .map_err(TelemetryCollectorError::Timebase)
    }

    fn flush_typing(&mut self) {
        if let Some(signal) = self.typing.take_signal() {
            self.signals.push(signal);
        }
    }
}
