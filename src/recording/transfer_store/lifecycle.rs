//! Recording temp media lifecycle operations.

use std::path::PathBuf;

use super::{RecordingTransferStore, RecordingTransferStoreError};
use crate::protocol::{DeliveryRoute, OperationAuthority};
use crate::recording::{
    recovery::{PendingRecordingTransfer, pending_recordings_for_reconnect},
    temp_media::{
        RecordingMediaMetadata, RecoveredTempMedia, StartupRecoveryDecision, TempMediaRecord,
        TempStoreError,
    },
};

use super::protection::protect_recording_media;

impl RecordingTransferStore {
    #[cfg(test)]
    pub(crate) fn start_recording_at(
        &self,
        recording_id: &str,
        now_ms: u64,
    ) -> Result<PathBuf, RecordingTransferStoreError> {
        self.start_recording_with_route_at(
            recording_id,
            OperationAuthority::ExtensionManaged {
                settings_revision: "legacy".to_owned(),
            },
            DeliveryRoute::ExtensionTransfer,
            now_ms,
        )
    }

    pub(crate) fn start_recording_with_route_at(
        &self,
        recording_id: &str,
        authority: OperationAuthority,
        delivery_route: DeliveryRoute,
        now_ms: u64,
    ) -> Result<PathBuf, RecordingTransferStoreError> {
        if self
            .disk
            .read_record(recording_id)
            .map_err(RecordingTransferStoreError::Disk)?
            .is_some()
        {
            return Err(RecordingTransferStoreError::InvalidState(
                TempStoreError::InvalidTransition,
            ));
        }
        let record = TempMediaRecord::start_writing_with_route(
            recording_id,
            authority,
            delivery_route,
            now_ms,
        )
        .map_err(RecordingTransferStoreError::InvalidState)?;
        self.disk
            .write_record(&record)
            .map_err(RecordingTransferStoreError::Disk)?;
        self.disk
            .recording_media_path(recording_id)
            .map_err(RecordingTransferStoreError::Disk)
    }

    pub(crate) fn finalize_recording_at(
        &self,
        recording_id: &str,
        metadata: RecordingMediaMetadata,
        now_ms: u64,
    ) -> Result<PendingRecordingTransfer, RecordingTransferStoreError> {
        let mut record = self
            .disk
            .read_record(recording_id)
            .map_err(RecordingTransferStoreError::Disk)?
            .ok_or(RecordingTransferStoreError::InvalidState(
                TempStoreError::InvalidTransition,
            ))?;
        let media_path = self
            .disk
            .recording_media_path(recording_id)
            .map_err(RecordingTransferStoreError::Disk)?;
        if !matches!(
            (&record.authority, &record.delivery_route),
            (
                Some(OperationAuthority::ExtensionManaged { .. }),
                Some(DeliveryRoute::ExtensionTransfer)
            )
        ) {
            return Err(RecordingTransferStoreError::InvalidState(
                TempStoreError::InvalidTransition,
            ));
        }
        let (total_bytes, sha256, media_protection) = protect_recording_media(&media_path)?;
        record
            .finalize_recording_with_protection(
                total_bytes,
                &sha256,
                metadata,
                Some(media_protection),
                now_ms,
            )
            .map_err(RecordingTransferStoreError::InvalidState)?;
        self.disk
            .write_record(&record)
            .map_err(RecordingTransferStoreError::Disk)?;
        let recovered = [RecoveredTempMedia {
            record,
            decision: StartupRecoveryDecision::ReportPending,
        }];
        pending_recordings_for_reconnect(&recovered)
            .map_err(RecordingTransferStoreError::Recovery)?
            .into_iter()
            .next()
            .ok_or(RecordingTransferStoreError::InvalidState(
                TempStoreError::InvalidTransition,
            ))
    }

    #[cfg(any(windows, test))]
    pub(crate) fn finalize_manual_recording_at(
        &self,
        recording_id: &str,
        metadata: RecordingMediaMetadata,
        now_ms: u64,
    ) -> Result<(), RecordingTransferStoreError> {
        let mut record = self
            .disk
            .read_record(recording_id)
            .map_err(RecordingTransferStoreError::Disk)?
            .ok_or(RecordingTransferStoreError::InvalidState(
                TempStoreError::InvalidTransition,
            ))?;
        if !matches!(
            (&record.authority, &record.delivery_route),
            (
                Some(OperationAuthority::Manual { .. }),
                Some(DeliveryRoute::LocalAsset { .. })
            )
        ) {
            return Err(RecordingTransferStoreError::InvalidState(
                TempStoreError::InvalidTransition,
            ));
        }
        let media_path = self
            .disk
            .recording_media_path(recording_id)
            .map_err(RecordingTransferStoreError::Disk)?;
        let (total_bytes, sha256, media_protection) = protect_recording_media(&media_path)?;
        record
            .finalize_recording_with_protection(
                total_bytes,
                &sha256,
                metadata,
                Some(media_protection),
                now_ms,
            )
            .map_err(RecordingTransferStoreError::InvalidState)?;
        self.disk
            .write_record(&record)
            .map_err(RecordingTransferStoreError::Disk)
    }

    pub(crate) fn recording_media_path(
        &self,
        recording_id: &str,
    ) -> Result<PathBuf, RecordingTransferStoreError> {
        self.disk
            .recording_media_path(recording_id)
            .map_err(RecordingTransferStoreError::Disk)
    }

    pub(crate) fn discard_recording(
        &self,
        recording_id: &str,
    ) -> Result<(), RecordingTransferStoreError> {
        self.delete_recording(recording_id)
    }
}
