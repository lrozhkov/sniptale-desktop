use super::{
    RecordingLifecycleService,
    messages::recording_started_event,
    progress_tick::progress_message,
    source::{recording_id_for_operation, recording_source_supported},
};
use crate::{
    protocol::{
        DeliveryRoute, OperationAuthority, RECORDING_MAX_TOTAL_BYTES, RecordingProgressStatus,
    },
    recording::{
        disk_guard::ensure_recording_disk_headroom,
        progress::RecordingProgressSnapshot,
        runtime::{RecordingRuntime, RecordingRuntimeError, RecordingRuntimeStartRequest},
        service_contract::{RecordingCommandEvent, RecordingStartPolicy, RecordingStartRequest},
        session::RecordingState,
    },
    telemetry::RecordingTelemetrySource,
};

impl<R, T> RecordingLifecycleService<R, T>
where
    R: RecordingRuntime,
    T: RecordingTelemetrySource,
{
    pub(super) fn start_recording_lifecycle(
        &mut self,
        request: &RecordingStartRequest,
        policy: RecordingStartPolicy,
    ) -> Result<Vec<RecordingCommandEvent>, RecordingRuntimeError> {
        if !matches!(
            (&request.authority, &request.delivery_route),
            (
                OperationAuthority::ExtensionManaged { .. },
                DeliveryRoute::ExtensionTransfer
            ) | (
                OperationAuthority::Manual { .. },
                DeliveryRoute::LocalAsset { .. }
            )
        ) {
            return Err(RecordingRuntimeError::UnsupportedMode);
        }
        if !recording_source_supported(&request.source) {
            return Err(RecordingRuntimeError::UnsupportedMode);
        }
        if self.session.state() != RecordingState::Idle {
            return Err(RecordingRuntimeError::InvalidState);
        }
        let controller_lease_id = request.delivery_session.controller_lease_id.as_deref();
        if matches!(
            request.authority,
            OperationAuthority::ExtensionManaged { .. }
        ) && controller_lease_id.is_none()
        {
            return Err(RecordingRuntimeError::InvalidState);
        }

        let recording_id = recording_id_for_operation(&request.operation_id);
        let media_path = self.transfer_store.recording_media_path(&recording_id)?;
        let space = (self.disk_space_probe)(&media_path)?;
        ensure_recording_disk_headroom(space, RECORDING_MAX_TOTAL_BYTES)?;
        let media_path = self.transfer_store.start_recording_with_route_at(
            &recording_id,
            request.authority.clone(),
            request.delivery_route.clone(),
            request.requested_at_epoch_ms,
        )?;
        let started = match self.runtime.start_recording(RecordingRuntimeStartRequest {
            request,
            recording_id: &recording_id,
            media_path: &media_path,
        }) {
            Ok(started) => started,
            Err(error) => {
                self.discard_recording_best_effort(&recording_id);
                return Err(error);
            }
        };
        let started_at_monotonic_ns = self.monotonic_clock.now_ns();
        self.session.start(&recording_id)?;
        self.session.begin_recording()?;
        self.telemetry_enabled = matches!(request.delivery_route, DeliveryRoute::ExtensionTransfer);
        if self.telemetry_enabled {
            if let Some(controller_lease_id) = controller_lease_id {
                self.telemetry.bind_controller_lease(controller_lease_id);
            }
            self.telemetry.start_recording_with_policy(
                policy.telemetry,
                &recording_id,
                started_at_monotonic_ns,
            );
        }
        self.timeline.start(request.requested_at_epoch_ms);
        self.start_policy = policy;
        self.open_editor = Some(request.open_editor);
        self.delivery_route = Some(request.delivery_route.clone());
        let initial_progress = self.progress.force_emit(
            request.requested_at_epoch_ms,
            RecordingProgressSnapshot {
                duration_ms: 0,
                status: RecordingProgressStatus::Recording,
            },
        );

        let mut messages = vec![RecordingCommandEvent::Started(recording_started_event(
            request,
            recording_id.clone(),
            started.effective_quality,
            started_at_monotonic_ns.to_string(),
        ))];
        messages.push(progress_message(&recording_id, initial_progress));
        Ok(messages)
    }
}
