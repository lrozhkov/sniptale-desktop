//! Manual-route plaintext reader port for frozen recording artifacts.

use super::{RecordingTransferStore, RecordingTransferStoreError};
use crate::{
    recording::temp_media::TempStoreError, transfer::protected_read::ProtectedMediaReader,
};

impl RecordingTransferStore {
    pub(crate) fn retry_rejected_desktop(&self) -> Result<(), RecordingTransferStoreError> {
        self.retry_rejected_desktop_at(super::system_epoch_ms())
    }

    /// Called only by the explicit resident recovery command; never on browser reconnect.
    pub(crate) fn retry_rejected_desktop_at(
        &self,
        now_ms: u64,
    ) -> Result<(), RecordingTransferStoreError> {
        let records = self
            .disk
            .scan_recovery(
                now_ms,
                self.config.writing_expiry,
                self.config.finalized_expiry,
            )
            .map_err(RecordingTransferStoreError::Disk)?;
        for item in records
            .into_iter()
            .filter(|item| {
                matches!(
                    (
                        &item.decision,
                        &item.record.authority,
                        &item.record.delivery_route
                    ),
                    (
                        crate::recording::temp_media::StartupRecoveryDecision::RetainRejected,
                        Some(crate::protocol::OperationAuthority::Manual { .. }),
                        Some(crate::protocol::DeliveryRoute::DesktopDelivery { .. })
                    )
                )
            })
            .take(16)
        {
            let mut record = item.record;
            record
                .retry_rejected_desktop(now_ms)
                .map_err(RecordingTransferStoreError::InvalidState)?;
            self.disk
                .write_record(&record)
                .map_err(RecordingTransferStoreError::Disk)?;
        }
        Ok(())
    }

    /// Opens a plaintext reader only for the exact Manual route frozen with this recording.
    #[cfg(any(windows, test))]
    pub(crate) fn manual_asset_reader(
        &self,
        recording_id: &str,
        asset_job_id: &str,
    ) -> Result<ProtectedMediaReader, RecordingTransferStoreError> {
        let record = self
            .disk
            .read_record(recording_id)
            .map_err(RecordingTransferStoreError::Disk)?
            .ok_or(RecordingTransferStoreError::InvalidState(
                TempStoreError::InvalidTransition,
            ))?;
        if !matches!(
            (&record.authority, &record.delivery_route),
            (
                Some(crate::protocol::OperationAuthority::Manual { .. }),
                Some(crate::protocol::DeliveryRoute::LocalAsset { asset_job_id: bound } | crate::protocol::DeliveryRoute::DesktopDelivery { asset_job_id: bound, .. })
            ) if bound == asset_job_id
        ) {
            return Err(RecordingTransferStoreError::InvalidState(
                TempStoreError::InvalidTransition,
            ));
        }
        let total_bytes = record
            .total_bytes
            .ok_or(RecordingTransferStoreError::InvalidState(
                TempStoreError::InvalidSize,
            ))?;
        let sha256 = record
            .sha256
            .ok_or(RecordingTransferStoreError::InvalidState(
                TempStoreError::InvalidSha256,
            ))?;
        let protection =
            record
                .media_protection
                .ok_or(RecordingTransferStoreError::InvalidState(
                    TempStoreError::InvalidTransition,
                ))?;
        let path = self
            .disk
            .recording_media_path(recording_id)
            .map_err(RecordingTransferStoreError::Disk)?;
        ProtectedMediaReader::open(path, &protection, total_bytes, sha256)
            .map_err(RecordingTransferStoreError::Transfer)
    }
}
