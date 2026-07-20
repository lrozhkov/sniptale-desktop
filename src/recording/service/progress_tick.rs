//! Recording lifecycle progress message cadence bridge.

use super::messages::recording_progress_event;
#[cfg(test)]
use super::{RecordingLifecycleService, messages::progress_status};
use crate::recording::service_contract::RecordingCommandEvent;

use super::super::progress::RecordingProgressSnapshot;
#[cfg(test)]
use super::super::runtime::{RecordingRuntime, RecordingRuntimeError};
#[cfg(test)]
use super::super::session::RecordingControl as SessionRecordingControl;
#[cfg(test)]
use crate::telemetry::RecordingTelemetrySource;

#[cfg(test)]
impl<R, T> RecordingLifecycleService<R, T>
where
    R: RecordingRuntime,
    T: RecordingTelemetrySource,
{
    pub(crate) fn progress_tick(
        &mut self,
        _controller_lease_id: &str,
        observed_epoch_ms: u64,
    ) -> Result<Vec<RecordingCommandEvent>, RecordingRuntimeError> {
        let Some(status) = progress_status(self.session.state()) else {
            return Ok(Vec::new());
        };
        let Some(recording_id) = self.session.recording_id().map(str::to_owned) else {
            return Ok(Vec::new());
        };
        let duration_ms = self.timeline.active_duration_ms(observed_epoch_ms);
        if duration_ms >= self.start_policy.max_duration_ms {
            self.session.apply_control(SessionRecordingControl::Stop)?;
            let observed_monotonic_ns = self.monotonic_clock.now_ns();
            self.timeline
                .record_control(SessionRecordingControl::Stop, observed_epoch_ms);
            let progress = self.progress.force_emit(
                observed_epoch_ms,
                RecordingProgressSnapshot {
                    duration_ms,
                    status: crate::protocol::RecordingProgressStatus::Stopping,
                },
            );
            let mut messages = vec![progress_message(&recording_id, progress)];
            self.telemetry
                .finish_recording(&recording_id, observed_monotonic_ns);
            messages.extend(self.finalize_recording_at(
                &recording_id,
                observed_epoch_ms,
                duration_ms,
            )?);
            return Ok(messages);
        }
        Ok(self
            .progress
            .observe(
                observed_epoch_ms,
                RecordingProgressSnapshot {
                    duration_ms,
                    status,
                },
            )
            .map(|progress| progress_message(&recording_id, progress))
            .into_iter()
            .collect())
    }
}

pub(super) fn progress_message(
    recording_id: &str,
    progress: RecordingProgressSnapshot,
) -> RecordingCommandEvent {
    RecordingCommandEvent::Progress(recording_progress_event(
        recording_id,
        progress.duration_ms,
        progress.status,
    ))
}
