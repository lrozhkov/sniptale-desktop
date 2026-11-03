//! Recording telemetry event collector.

mod typing;

#[cfg(test)]
use super::sanitize::{KeyCode, KeyModifiers, sanitize_key};
use super::{
    sanitize::SanitizedKeySignal,
    snapshot::{
        RecordingTelemetryActionEvent, RecordingTelemetrySignal,
        SanitizedRecordingTelemetrySnapshot,
    },
    timebase::{MediaTimebase, TimebaseError},
};
use typing::TypingSpanAccumulator;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TelemetryCollectorError {
    RecordingEvents,
    HistoryBackpressure,
    CursorTrajectory,
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
    open_pause_started_monotonic_ns: Option<u64>,
    action_events: Vec<RecordingTelemetryActionEvent>,
    signals: Vec<RecordingTelemetrySignal>,
    typing: TypingSpanAccumulator,
    history_incomplete: bool,
}

impl RecordingTelemetryCollector {
    pub(crate) fn start_recording(&mut self, recording_id: &str, started_at_monotonic_ns: u64) {
        self.session = Some(RecordingTelemetryCollectionSession {
            recording_id: recording_id.to_owned(),
            timebase: MediaTimebase::new(started_at_monotonic_ns),
            open_pause_started_monotonic_ns: None,
            action_events: Vec::new(),
            signals: Vec::new(),
            typing: TypingSpanAccumulator::default(),
            history_incomplete: false,
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
            SanitizedKeySignal::Typing { count } => {
                if let Some(completed) = session.typing.record(time_ms, count) {
                    session.push_signal(completed)?;
                }
            }
            SanitizedKeySignal::KeyLabel(_) | SanitizedKeySignal::Shortcut(_) => {
                session.flush_typing()?;
                if let Some(action) =
                    RecordingTelemetryActionEvent::from_key_signal(signal, time_ms)
                {
                    if session.action_events.len() == 2048 {
                        session.history_incomplete = true;
                        return Err(TelemetryCollectorError::HistoryBackpressure);
                    }
                    session.action_events.push(action);
                }
            }
            SanitizedKeySignal::Ignored => {}
        }
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
        session.push_signal(RecordingTelemetrySignal::telemetry_warning(
            source, code, time_ms,
        ))
    }

    pub(crate) fn record_pause_start(
        &mut self,
        recording_id: &str,
        start_monotonic_ns: u64,
    ) -> Result<(), TelemetryCollectorError> {
        let session = self.session_for(recording_id)?;
        let flushed = session.flush_typing();
        session
            .timebase
            .pause(start_monotonic_ns)
            .map_err(TelemetryCollectorError::Timebase)?;
        session.open_pause_started_monotonic_ns = Some(start_monotonic_ns);
        flushed
    }

    pub(crate) fn record_pause_end(
        &mut self,
        recording_id: &str,
        end_monotonic_ns: u64,
    ) -> Result<(), TelemetryCollectorError> {
        let session = self.session_for(recording_id)?;
        let Some(_) = session.open_pause_started_monotonic_ns else {
            return Ok(());
        };
        session
            .timebase
            .resume(end_monotonic_ns)
            .map_err(TelemetryCollectorError::Timebase)?;
        session.open_pause_started_monotonic_ns = None;
        Ok(())
    }

    /// Drains a bounded accepted prefix without closing an active typing span or recording.
    #[cfg(test)]
    pub(crate) fn drain_pending_for_recording(
        &mut self,
        recording_id: &str,
    ) -> Result<SanitizedRecordingTelemetrySnapshot, TelemetryCollectorError> {
        Ok(self.session_for(recording_id)?.drain_pending())
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
        let _ = duration_ms;
        let _ = session.flush_typing();
        Some(session.drain_pending())
    }
}

impl RecordingTelemetryCollectionSession {
    fn media_time_ms(&mut self, monotonic_ns: u64) -> Result<u64, TelemetryCollectorError> {
        self.timebase
            .elapsed_media_ms(monotonic_ns)
            .map_err(TelemetryCollectorError::Timebase)
    }

    fn flush_typing(&mut self) -> Result<(), TelemetryCollectorError> {
        if let Some(signal) = self.typing.take_signal() {
            self.push_signal(signal)?;
        }
        Ok(())
    }

    fn push_signal(
        &mut self,
        signal: RecordingTelemetrySignal,
    ) -> Result<(), TelemetryCollectorError> {
        // Reserve a fixed slot for a loss marker outside the accepted prefix.
        if self.signals.len() == 511 {
            self.history_incomplete = true;
            return Err(TelemetryCollectorError::HistoryBackpressure);
        }
        self.signals.push(signal);
        Ok(())
    }

    fn drain_pending(&mut self) -> SanitizedRecordingTelemetrySnapshot {
        let mut snapshot = SanitizedRecordingTelemetrySnapshot::new(
            std::mem::take(&mut self.action_events),
            std::mem::take(&mut self.signals),
        );
        snapshot.history_incomplete = self.history_incomplete;
        if self.history_incomplete {
            snapshot
                .signals
                .push(RecordingTelemetrySignal::telemetry_warning(
                    "keyboard-hook",
                    "events-dropped",
                    0,
                ));
        }
        snapshot
    }
}
