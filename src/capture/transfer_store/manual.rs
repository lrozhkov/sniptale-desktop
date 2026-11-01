//! Manual-route plaintext reader port for frozen screenshot artifacts.

use super::{ScreenshotTransferStore, ScreenshotTransferStoreError};
use crate::transfer::protected_read::ProtectedMediaReader;

impl ScreenshotTransferStore {
    pub(crate) fn retry_rejected_desktop(&self) -> Result<(), ScreenshotTransferStoreError> {
        self.retry_rejected_desktop_at(super::service::system_epoch_ms())
    }

    /// Called only by the explicit resident recovery command; never on browser reconnect.
    pub(crate) fn retry_rejected_desktop_at(
        &self,
        now_ms: u64,
    ) -> Result<(), ScreenshotTransferStoreError> {
        let records = self.disk.scan_records()?;
        for mut record in records
            .into_iter()
            .filter(|record| {
                matches!(
                    (&record.state, &record.authority, &record.delivery_route),
                    (
                        super::super::transfer_record::ScreenshotTransferState::Rejected,
                        Some(crate::protocol::OperationAuthority::Manual { .. }),
                        Some(crate::protocol::DeliveryRoute::DesktopDelivery { .. })
                    )
                )
            })
            .take(16)
        {
            record.retry_rejected_desktop(now_ms)?;
            self.disk.write_record(&record)?;
        }
        Ok(())
    }

    /// Opens a plaintext reader only for the exact Manual route frozen with this capture.
    #[cfg(any(windows, test))]
    pub(crate) fn manual_asset_reader(
        &self,
        capture_id: &str,
        asset_job_id: &str,
    ) -> Result<ProtectedMediaReader, ScreenshotTransferStoreError> {
        let record = self
            .disk
            .read_record(capture_id)?
            .ok_or(ScreenshotTransferStoreError::InvalidMetadata)?;
        if !matches!(
            (&record.authority, &record.delivery_route),
            (
                Some(crate::protocol::OperationAuthority::Manual { .. }),
                Some(crate::protocol::DeliveryRoute::LocalAsset { asset_job_id: bound } | crate::protocol::DeliveryRoute::DesktopDelivery { asset_job_id: bound, .. })
            ) if bound == asset_job_id
        ) {
            return Err(ScreenshotTransferStoreError::InvalidMetadata);
        }
        let total_bytes = record
            .total_bytes
            .ok_or(ScreenshotTransferStoreError::InvalidMetadata)?;
        let sha256 = record
            .sha256
            .ok_or(ScreenshotTransferStoreError::InvalidMetadata)?;
        let protection = record
            .media_protection
            .ok_or(ScreenshotTransferStoreError::InvalidMetadata)?;
        let path = self.disk.screenshot_media_path(capture_id)?;
        ProtectedMediaReader::open(path, &protection, total_bytes, sha256)
            .map_err(ScreenshotTransferStoreError::Transfer)
    }
}
