//! Manual-route plaintext reader port for frozen recording artifacts.

use super::{RecordingTransferStore, RecordingTransferStoreError};
use crate::{
    recording::temp_media::TempStoreError, transfer::protected_read::ProtectedMediaReader,
};

impl RecordingTransferStore {
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
                Some(crate::protocol::DeliveryRoute::LocalAsset { asset_job_id: bound })
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
