//! Explicit best-effort telemetry warning recording.

use super::RuntimeRecordingTelemetrySource;

impl RuntimeRecordingTelemetrySource {
    pub(super) fn record_telemetry_warning_best_effort(
        &mut self,
        recording_id: &str,
        source: &'static str,
        code: &'static str,
        monotonic_ns: u64,
    ) {
        self.collector
            .record_telemetry_warning(recording_id, source, code, monotonic_ns)
            .unwrap_or(());
    }
}
