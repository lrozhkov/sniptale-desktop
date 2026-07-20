//! Screenshot transfer cache service side effects for ack, reject, chunking, and open.

use std::{
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use super::super::transfer_disk::ScreenshotTransferDiskStore;
#[cfg(any(windows, test))]
use super::super::transfer_record::{FinalizedScreenshotTransfer, PreparedScreenshotTransfer};
use super::super::transfer_record::{
    RecoveryDecision, ScreenshotTransferRecord, ScreenshotTransferRecordError,
};
#[cfg(any(windows, test))]
use super::ScreenshotTransferChunk;
#[cfg(any(windows, test))]
use super::protection::protect_screenshot_media_file;
use super::{ScreenshotTransferStore, ScreenshotTransferStoreConfig, ScreenshotTransferStoreError};
#[cfg(any(windows, test))]
use crate::transfer::{
    chunks::MediaTransferKind, error::TransferError, protected_key::ProtectedMediaKey,
    protected_read::read_protected_media_chunk,
};

#[cfg(any(windows, test))]
const SCREENSHOT_CHUNK_ENVELOPE_UTF8_BYTES: usize = 512;

impl ScreenshotTransferStore {
    pub(crate) fn open(
        root: impl Into<PathBuf>,
        config: ScreenshotTransferStoreConfig,
    ) -> Result<Self, ScreenshotTransferStoreError> {
        Ok(Self {
            disk: ScreenshotTransferDiskStore::open(root)?,
            config,
        })
    }

    #[cfg(test)]
    pub(crate) fn open_with_default_config(
        root: impl Into<PathBuf>,
    ) -> Result<Self, ScreenshotTransferStoreError> {
        Self::open(root, ScreenshotTransferStoreConfig::default())
    }

    pub(crate) fn open_default() -> Result<Self, ScreenshotTransferStoreError> {
        let local_app_data = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .ok_or(ScreenshotTransferStoreError::Io)?;
        Self::open(
            local_app_data
                .join("Sniptale")
                .join("NativeAgent")
                .join("temp-media")
                .join("screenshots"),
            ScreenshotTransferStoreConfig::default(),
        )
    }

    #[cfg(test)]
    pub(crate) fn write_finalized_capture_at(
        &self,
        transfer: FinalizedScreenshotTransfer,
        now_ms: u64,
    ) -> Result<(), ScreenshotTransferStoreError> {
        let record = ScreenshotTransferRecord::finalized(transfer, now_ms)?;
        self.disk.write_record(&record)
    }

    #[cfg(any(windows, test))]
    pub(crate) fn start_screenshot_at(
        &self,
        transfer: PreparedScreenshotTransfer,
        now_ms: u64,
    ) -> Result<(), ScreenshotTransferStoreError> {
        if self.disk.read_record(&transfer.capture_id)?.is_some() {
            return Err(ScreenshotTransferStoreError::InvalidMetadata);
        }
        let record = ScreenshotTransferRecord::writing(transfer, now_ms)?;
        self.disk.write_record(&record)
    }

    #[cfg(any(windows, test))]
    pub(crate) fn finalize_prepared_screenshot_at(
        &self,
        transfer: FinalizedScreenshotTransfer,
        now_ms: u64,
    ) -> Result<(), ScreenshotTransferStoreError> {
        let Some(prepared) = self.disk.read_record(&transfer.capture_id)? else {
            return Err(ScreenshotTransferStoreError::InvalidMetadata);
        };
        if !prepared.matches_prepared_transfer(&transfer) {
            return Err(ScreenshotTransferStoreError::InvalidMetadata);
        }
        let record = ScreenshotTransferRecord::finalized(transfer, now_ms)?;
        self.disk.write_record(&record)
    }

    #[cfg(any(windows, test))]
    pub(crate) fn discard_screenshot(
        &self,
        capture_id: &str,
    ) -> Result<(), ScreenshotTransferStoreError> {
        self.disk.delete_capture(capture_id)
    }

    #[cfg(any(windows, test))]
    pub(crate) fn protect_screenshot_media(
        &self,
        capture_id: &str,
    ) -> Result<(u64, String, ProtectedMediaKey), ScreenshotTransferStoreError> {
        let path = self.disk.screenshot_media_path(capture_id)?;
        protect_screenshot_media_file(&path)
    }

    #[cfg(any(windows, test))]
    pub(crate) fn screenshot_chunk_at(
        &self,
        capture_id: &str,
        chunk_index: u32,
        now_ms: u64,
    ) -> Result<Option<ScreenshotTransferChunk>, ScreenshotTransferStoreError> {
        let Some(mut record) = self.pending_record(capture_id, now_ms)? else {
            return Ok(None);
        };
        let total_bytes = record
            .total_bytes
            .ok_or(ScreenshotTransferRecordError::InvalidState)?;
        record
            .validate_chunk_request(u64::from(chunk_index))
            .map_err(screenshot_record_store_error)?;
        let path = self.disk.screenshot_media_path(capture_id)?;
        let chunk = match &record.media_protection {
            Some(protection) => read_protected_media_chunk(
                MediaTransferKind::Screenshot,
                &path,
                protection,
                total_bytes,
                u64::from(chunk_index),
                SCREENSHOT_CHUNK_ENVELOPE_UTF8_BYTES,
            ),
            None => Err(TransferError::ProtectedMedia),
        };
        let chunk = chunk.map_err(ScreenshotTransferStoreError::Transfer)?;
        let is_complete = record
            .record_chunk_read(u64::from(chunk_index), now_ms)
            .map_err(screenshot_record_store_error)?;
        self.disk.write_record(&record)?;
        Ok(Some(ScreenshotTransferChunk { chunk, is_complete }))
    }

    #[cfg(any(windows, test))]
    pub(crate) fn ack_screenshot(
        &self,
        capture_id: &str,
    ) -> Result<bool, ScreenshotTransferStoreError> {
        self.ack_screenshot_at(capture_id, system_epoch_ms())
    }

    #[cfg(any(windows, test))]
    pub(crate) fn ack_screenshot_at(
        &self,
        capture_id: &str,
        now_ms: u64,
    ) -> Result<bool, ScreenshotTransferStoreError> {
        let Some(mut record) = self.pending_record(capture_id, now_ms)? else {
            return Ok(false);
        };
        record.ack(now_ms).map_err(screenshot_record_store_error)?;
        self.disk.delete_capture(capture_id)?;
        Ok(true)
    }

    #[cfg(any(windows, test))]
    pub(crate) fn reject_screenshot(
        &self,
        capture_id: &str,
        retain_for_diagnostics: bool,
    ) -> Result<bool, ScreenshotTransferStoreError> {
        self.reject_screenshot_at(capture_id, retain_for_diagnostics, system_epoch_ms())
    }

    #[cfg(any(windows, test))]
    pub(crate) fn reject_screenshot_at(
        &self,
        capture_id: &str,
        retain_for_diagnostics: bool,
        now_ms: u64,
    ) -> Result<bool, ScreenshotTransferStoreError> {
        let Some(mut record) = self.pending_record(capture_id, now_ms)? else {
            return Ok(false);
        };
        if retain_for_diagnostics {
            record
                .reject(
                    now_ms,
                    Some(now_ms.saturating_add(self.config.rejected_retention)),
                )
                .map_err(screenshot_record_store_error)?;
            self.disk.write_record(&record)?;
        } else {
            record
                .reject(now_ms, None)
                .map_err(screenshot_record_store_error)?;
            self.disk.delete_capture(capture_id)?;
        }
        Ok(true)
    }

    #[cfg(any(windows, test))]
    pub(crate) fn screenshot_media_path(
        &self,
        capture_id: &str,
    ) -> Result<PathBuf, ScreenshotTransferStoreError> {
        self.disk.screenshot_media_path(capture_id)
    }

    pub(super) fn recovery_decision(
        &self,
        record: &ScreenshotTransferRecord,
        now_ms: u64,
    ) -> Result<RecoveryDecision, ScreenshotTransferRecordError> {
        record.recovery_decision(
            now_ms,
            self.config.writing_expiry,
            self.config.finalized_expiry,
        )
    }

    #[cfg(any(windows, test))]
    fn pending_record(
        &self,
        capture_id: &str,
        now_ms: u64,
    ) -> Result<Option<ScreenshotTransferRecord>, ScreenshotTransferStoreError> {
        let Some(record) = self.disk.read_record(capture_id)? else {
            return Ok(None);
        };
        match self.recovery_decision(&record, now_ms)? {
            RecoveryDecision::ReportPending => Ok(Some(record)),
            RecoveryDecision::Delete => {
                self.disk.delete_capture(capture_id)?;
                Ok(None)
            }
            RecoveryDecision::KeepWriting | RecoveryDecision::RetainRejected => Ok(None),
        }
    }
}

fn screenshot_record_store_error(
    error: ScreenshotTransferRecordError,
) -> ScreenshotTransferStoreError {
    match error {
        ScreenshotTransferRecordError::Transfer(error) => {
            ScreenshotTransferStoreError::Transfer(error)
        }
        error => ScreenshotTransferStoreError::InvalidRecord(error),
    }
}

pub(super) fn system_epoch_ms() -> u64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => u64::try_from(duration.as_millis()).unwrap_or(u64::MAX),
        Err(_) => 0,
    }
}
