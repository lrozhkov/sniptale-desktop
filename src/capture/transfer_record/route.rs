//! Immutable delivery-route metadata for screenshot transfer records.

use super::{ScreenshotTransferRecord, ScreenshotTransferRecordError, ScreenshotTransferState};
use crate::{
    capture::coordinates::PhysicalRect,
    protocol::{DeliveryRoute, OperationAuthority, ScreenshotMode},
    transfer::{chunks::chunk_count, protected_key::ProtectedMediaKey},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PreparedScreenshotTransfer {
    pub(crate) capture_id: String,
    pub(crate) mode: ScreenshotMode,
    pub(crate) filename: String,
    pub(crate) bounds: PhysicalRect,
    pub(crate) authority: OperationAuthority,
    pub(crate) delivery_route: DeliveryRoute,
    pub(crate) open_editor: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FinalizedScreenshotTransfer {
    pub(crate) capture_id: String,
    pub(crate) mode: ScreenshotMode,
    pub(crate) filename: String,
    pub(crate) bounds: PhysicalRect,
    pub(crate) authority: OperationAuthority,
    pub(crate) delivery_route: DeliveryRoute,
    pub(crate) total_bytes: u64,
    pub(crate) sha256: String,
    pub(crate) media_protection: Option<ProtectedMediaKey>,
    pub(crate) open_editor: bool,
}

impl ScreenshotTransferRecord {
    pub(crate) fn writing(
        transfer: PreparedScreenshotTransfer,
        now_ms: u64,
    ) -> Result<Self, ScreenshotTransferRecordError> {
        let record = Self {
            capture_id: transfer.capture_id,
            state: ScreenshotTransferState::Writing,
            created_at_ms: now_ms,
            updated_at_ms: now_ms,
            mode: transfer.mode,
            filename: transfer.filename,
            route_schema: Some(1),
            authority: Some(transfer.authority),
            delivery_route: Some(transfer.delivery_route),
            x: transfer.bounds.x,
            y: transfer.bounds.y,
            width: transfer.bounds.width,
            height: transfer.bounds.height,
            total_bytes: None,
            sha256: None,
            next_missing_chunk_index: 0,
            media_protection: None,
            open_editor: transfer.open_editor,
            retain_rejected_until_ms: None,
        };
        record.validate()?;
        Ok(record)
    }

    pub(crate) fn finalized(
        transfer: FinalizedScreenshotTransfer,
        now_ms: u64,
    ) -> Result<Self, ScreenshotTransferRecordError> {
        let chunks =
            chunk_count(transfer.total_bytes).map_err(ScreenshotTransferRecordError::Transfer)?;
        let _ = u32::try_from(chunks).map_err(|_| ScreenshotTransferRecordError::InvalidState)?;
        let record = Self {
            capture_id: transfer.capture_id,
            state: ScreenshotTransferState::Finalized,
            created_at_ms: now_ms,
            updated_at_ms: now_ms,
            mode: transfer.mode,
            filename: transfer.filename,
            route_schema: Some(1),
            authority: Some(transfer.authority),
            delivery_route: Some(transfer.delivery_route),
            x: transfer.bounds.x,
            y: transfer.bounds.y,
            width: transfer.bounds.width,
            height: transfer.bounds.height,
            total_bytes: Some(transfer.total_bytes),
            sha256: Some(transfer.sha256),
            next_missing_chunk_index: 0,
            media_protection: transfer.media_protection,
            open_editor: transfer.open_editor,
            retain_rejected_until_ms: None,
        };
        record.validate()?;
        Ok(record)
    }

    pub(crate) fn matches_prepared_transfer(&self, transfer: &FinalizedScreenshotTransfer) -> bool {
        self.state == ScreenshotTransferState::Writing
            && self.capture_id == transfer.capture_id
            && self.mode == transfer.mode
            && self.filename == transfer.filename
            && self.x == transfer.bounds.x
            && self.y == transfer.bounds.y
            && self.width == transfer.bounds.width
            && self.height == transfer.bounds.height
            && self.authority.as_ref() == Some(&transfer.authority)
            && self.delivery_route.as_ref() == Some(&transfer.delivery_route)
            && self.open_editor == transfer.open_editor
    }
}
