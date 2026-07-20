use super::super::runtime::{RecordingRuntime, RecordingRuntimeError, RecordingRuntimeStopRequest};
use super::{
    RecordingLifecycleService,
    messages::{recording_metadata, recording_stopped_event},
    progress_tick::progress_message,
};
use crate::{
    protocol::{DeliveryRoute, RecordingProgressStatus},
    recording::progress::RecordingProgressSnapshot,
    recording::service_contract::{ManualRecordingFinalizedEvent, RecordingCommandEvent},
    telemetry::{RecordingTelemetrySource, SanitizedRecordingTelemetrySnapshot},
};

impl<R, T> RecordingLifecycleService<R, T>
where
    R: RecordingRuntime,
    T: RecordingTelemetrySource,
{
    pub(super) fn finalize_recording_at(
        &mut self,
        recording_id: &str,
        requested_at_epoch_ms: u64,
        duration_ms: u64,
    ) -> Result<Vec<RecordingCommandEvent>, RecordingRuntimeError> {
        self.session.mark_finalizing()?;
        let finalizing_progress = self.progress.force_emit(
            requested_at_epoch_ms,
            RecordingProgressSnapshot {
                duration_ms,
                status: RecordingProgressStatus::Finalizing,
            },
        );
        #[cfg(test)]
        let media_path = self.transfer_store.recording_media_path(recording_id)?;
        let artifact = match self.runtime.stop_recording(RecordingRuntimeStopRequest {
            recording_id,
            #[cfg(test)]
            media_path: &media_path,
            #[cfg(test)]
            requested_at_epoch_ms,
            duration_ms,
        }) {
            Ok(artifact) => artifact,
            Err(error) => {
                self.discard_and_reset(recording_id);
                return Err(error);
            }
        };
        let metadata =
            match recording_metadata(recording_id, artifact, self.open_editor.unwrap_or(false)) {
                Ok(metadata) => metadata,
                Err(error) => {
                    self.discard_and_reset(recording_id);
                    return Err(error);
                }
            };
        let route = self.delivery_route.clone();
        let terminal = match route {
            Some(DeliveryRoute::ExtensionTransfer) => self
                .transfer_store
                .finalize_recording_at(recording_id, metadata, requested_at_epoch_ms)
                .map(|pending| {
                    RecordingCommandEvent::Stopped(recording_stopped_event(
                        pending,
                        self.telemetry
                            .snapshot_for_recording(recording_id, duration_ms)
                            .map(SanitizedRecordingTelemetrySnapshot::into_protocol),
                    ))
                }),
            Some(DeliveryRoute::LocalAsset { asset_job_id }) => self
                .transfer_store
                .finalize_manual_recording_at(recording_id, metadata, requested_at_epoch_ms)
                .map(|()| {
                    RecordingCommandEvent::ManualFinalized(ManualRecordingFinalizedEvent {
                        recording_id: recording_id.to_owned(),
                        asset_job_id,
                    })
                }),
            None => Err(crate::recording::RecordingTransferStoreError::InvalidState(
                crate::recording::temp_media::TempStoreError::InvalidTransition,
            )),
        };
        let terminal = match terminal {
            Ok(event) => event,
            Err(error) => {
                self.discard_and_reset(recording_id);
                return Err(error.into());
            }
        };
        self.session.mark_transferring()?;
        self.session.complete()?;
        self.reset();
        let mut messages = vec![progress_message(recording_id, finalizing_progress)];
        messages.push(terminal);
        Ok(messages)
    }
}
