//! Reconnect reporting projection for recovered screenshots.

use super::{
    super::transfer_record::{RecoveryDecision, ScreenshotTransferRecordError},
    ScreenshotTransferStore, ScreenshotTransferStoreError,
    service::system_epoch_ms,
};
use crate::{protocol::ScreenshotMode, transfer::chunks::chunk_count};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PendingScreenshotTransfer {
    pub(crate) capture_id: String,
    pub(crate) mode: ScreenshotMode,
    pub(crate) filename: String,
    pub(crate) total_bytes: u64,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) sha256: String,
    pub(crate) chunk_count: u32,
    pub(crate) open_editor: bool,
    pub(crate) captured_at_epoch_ms: u64,
}

impl ScreenshotTransferStore {
    pub(crate) fn pending_screenshots_for_reconnect(
        &self,
    ) -> Result<Vec<PendingScreenshotTransfer>, ScreenshotTransferStoreError> {
        self.pending_screenshots_for_reconnect_at(system_epoch_ms())
    }

    pub(crate) fn pending_screenshots_for_reconnect_at(
        &self,
        now_ms: u64,
    ) -> Result<Vec<PendingScreenshotTransfer>, ScreenshotTransferStoreError> {
        let mut pending = Vec::new();
        for record in self.disk.scan_records()? {
            if self.recovery_decision(&record, now_ms)? != RecoveryDecision::ReportPending {
                continue;
            }
            if !matches!(
                (&record.authority, &record.delivery_route),
                (None, None)
                    | (
                        Some(crate::protocol::OperationAuthority::ExtensionManaged { .. }),
                        Some(crate::protocol::DeliveryRoute::ExtensionTransfer),
                    )
            ) {
                continue;
            }
            let total_bytes = record
                .total_bytes
                .ok_or(ScreenshotTransferRecordError::InvalidState)?;
            let chunks =
                chunk_count(total_bytes).map_err(ScreenshotTransferStoreError::Transfer)?;
            pending.push(PendingScreenshotTransfer {
                capture_id: record.capture_id,
                mode: record.mode,
                filename: record.filename,
                total_bytes,
                width: record.width,
                height: record.height,
                sha256: record
                    .sha256
                    .ok_or(ScreenshotTransferRecordError::InvalidState)?,
                chunk_count: u32::try_from(chunks)
                    .map_err(|_| ScreenshotTransferRecordError::InvalidState)?,
                open_editor: record.open_editor,
                captured_at_epoch_ms: record.created_at_ms,
            });
        }
        Ok(pending)
    }
}
