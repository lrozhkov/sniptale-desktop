use super::{
    RecordingLifecycleService,
    control::{recording_control, signal_runtime_control},
    messages::progress_status,
    progress_tick::progress_message,
};
use crate::{
    protocol::DeliveryRoute,
    recording::{
        progress::RecordingProgressSnapshot,
        runtime::{RecordingRuntime, RecordingRuntimeError},
        service_contract::{RecordingCommandEvent, RecordingControlRequest},
        session::RecordingControl as SessionRecordingControl,
    },
    telemetry::RecordingTelemetrySource,
};

impl<R, T> RecordingLifecycleService<R, T>
where
    R: RecordingRuntime,
    T: RecordingTelemetrySource,
{
    pub(super) fn control_recording_lifecycle(
        &mut self,
        request: &RecordingControlRequest,
    ) -> Result<Vec<RecordingCommandEvent>, RecordingRuntimeError> {
        if self.session.recording_id() != Some(request.recording_id.as_str()) {
            return Err(RecordingRuntimeError::InvalidState);
        }
        let controller_lease_id = request.delivery_session.controller_lease_id.as_deref();
        if matches!(self.delivery_route, Some(DeliveryRoute::ExtensionTransfer))
            && controller_lease_id.is_none()
        {
            return Err(RecordingRuntimeError::InvalidState);
        }

        let control = recording_control(&request.control);
        let previous_session = self.session.clone();
        self.session.apply_control(control)?;
        let observed_monotonic_ns = self.monotonic_clock.now_ns();
        if self.telemetry_enabled
            && let Some(controller_lease_id) = controller_lease_id
        {
            self.telemetry.bind_controller_lease(controller_lease_id);
        }

        if matches!(control, SessionRecordingControl::Cancel) {
            if let Err(error) = self.runtime.cancel_recording(&request.recording_id) {
                self.session = previous_session;
                return Err(error);
            }
            if let Err(error) = self.transfer_store.discard_recording(&request.recording_id) {
                self.session = previous_session;
                return Err(error.into());
            }
            if self.telemetry_enabled {
                self.telemetry
                    .cancel_recording(&request.recording_id, observed_monotonic_ns);
            }
            self.reset();
            return Ok(Vec::new());
        }

        if let Err(error) =
            signal_runtime_control(&mut self.runtime, control, &request.recording_id)
        {
            self.session = previous_session;
            return Err(error);
        }

        self.timeline
            .record_control(control, request.requested_at_epoch_ms);
        let duration_ms = self
            .timeline
            .active_duration_ms(request.requested_at_epoch_ms);
        let Some(status) = progress_status(self.session.state()) else {
            return Ok(Vec::new());
        };
        let progress = RecordingProgressSnapshot {
            duration_ms,
            status,
        };
        let mut messages = self
            .progress
            .observe(request.requested_at_epoch_ms, progress)
            .into_iter()
            .map(|progress| progress_message(&request.recording_id, progress))
            .collect::<Vec<_>>();
        if matches!(control, SessionRecordingControl::Stop) {
            if self.telemetry_enabled {
                self.telemetry
                    .finish_recording(&request.recording_id, observed_monotonic_ns);
            }
            messages.extend(self.finalize_recording_at(
                &request.recording_id,
                request.requested_at_epoch_ms,
                duration_ms,
            )?);
        } else if matches!(control, SessionRecordingControl::Pause) {
            if self.telemetry_enabled {
                self.telemetry
                    .pause_recording(&request.recording_id, observed_monotonic_ns);
            }
        } else if matches!(control, SessionRecordingControl::Resume) && self.telemetry_enabled {
            self.telemetry
                .resume_recording(&request.recording_id, observed_monotonic_ns);
        }
        Ok(messages)
    }
}
