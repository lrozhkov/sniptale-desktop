//! Typing span accumulation for sanitized telemetry.

use crate::telemetry::snapshot::RecordingTelemetrySignal;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct TypingSpanAccumulator {
    start_time_ms: Option<u64>,
    end_time_ms: Option<u64>,
    count: u16,
}

impl TypingSpanAccumulator {
    pub(super) fn record(&mut self, time_ms: u64, count: u16) {
        if self.start_time_ms.is_none() {
            self.start_time_ms = Some(time_ms);
        }
        self.end_time_ms = Some(time_ms);
        self.count = self.count.saturating_add(count);
    }

    pub(super) fn flush_at(&mut self, time_ms: u64) {
        if self.start_time_ms.is_some() {
            self.end_time_ms = Some(time_ms);
        }
    }

    pub(super) fn take_signal(&mut self) -> Option<RecordingTelemetrySignal> {
        if self.start_time_ms.is_some() && self.end_time_ms.is_none() {
            self.end_time_ms = self.start_time_ms;
        }
        let signal = match (self.start_time_ms, self.end_time_ms) {
            (Some(start_time_ms), Some(end_time_ms)) if self.count > 0 => Some(
                RecordingTelemetrySignal::typing_span(self.count, start_time_ms, end_time_ms),
            ),
            _ => None,
        };
        if signal.is_some() {
            *self = Self::default();
        }
        signal
    }
}
