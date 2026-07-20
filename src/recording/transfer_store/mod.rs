//! Recording transfer cache side effects for reconnect, chunk, ack, and reject.
mod lifecycle;
mod maintenance;
#[cfg(any(windows, test))]
mod manual;
mod protection;

use std::{
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(any(windows, test))]
use crate::transfer::{
    chunks::{EncodedChunk, MediaTransferKind},
    error::TransferError,
    lifecycle::cleanup_recovery_decision,
    protected_read::read_protected_media_chunk,
};

use super::{
    recovery::{
        PendingRecordingTransfer, RecordingRecoveryError, pending_recordings_for_reconnect,
    },
    temp_media::{
        StartupRecoveryDecision, TempMediaDiskStore, TempMediaRecord, TempStoreDiskError,
        TempStoreError,
    },
};

const DEFAULT_INCOMPLETE_TTL_MS: u64 = 60 * 60 * 1_000;
const DEFAULT_FINALIZED_TTL_MS: u64 = 14 * 24 * 60 * 60 * 1_000;
const DEFAULT_REJECTED_TTL_MS: u64 = 24 * 60 * 60 * 1_000;
#[cfg(any(windows, test))]
const RECORDING_CHUNK_ENVELOPE_UTF8_BYTES: usize = 512;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RecordingTransferStoreConfig {
    pub(crate) writing_expiry: u64,
    pub(crate) finalized_expiry: u64,
    pub(crate) rejected_retention: u64,
}

impl Default for RecordingTransferStoreConfig {
    fn default() -> Self {
        Self {
            writing_expiry: DEFAULT_INCOMPLETE_TTL_MS,
            finalized_expiry: DEFAULT_FINALIZED_TTL_MS,
            rejected_retention: DEFAULT_REJECTED_TTL_MS,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct RecordingTransferStore {
    disk: TempMediaDiskStore,
    config: RecordingTransferStoreConfig,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct StartupRecoverySummary {
    pub(crate) pending_recordings: usize,
    pub(crate) keep_writing: usize,
    pub(crate) retained_rejected: usize,
    pub(crate) deleted: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum RecordingTransferStoreError {
    Disk(TempStoreDiskError),
    Recovery(RecordingRecoveryError),
    InvalidState(TempStoreError),
    Protection,
    #[cfg(any(windows, test))]
    Transfer(TransferError),
}

impl RecordingTransferStore {
    pub(crate) fn open(
        root: impl Into<PathBuf>,
        config: RecordingTransferStoreConfig,
    ) -> Result<Self, RecordingTransferStoreError> {
        Ok(Self {
            disk: TempMediaDiskStore::open(root).map_err(RecordingTransferStoreError::Disk)?,
            config,
        })
    }

    #[cfg(test)]
    pub(crate) fn open_with_default_config(
        root: impl Into<PathBuf>,
    ) -> Result<Self, RecordingTransferStoreError> {
        Self::open(root, RecordingTransferStoreConfig::default())
    }

    pub(crate) fn open_default() -> Result<Self, RecordingTransferStoreError> {
        let local_app_data = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .ok_or(RecordingTransferStoreError::Disk(TempStoreDiskError::Io))?;
        Self::open(
            local_app_data
                .join("Sniptale")
                .join("NativeAgent")
                .join("temp-media"),
            RecordingTransferStoreConfig::default(),
        )
    }

    pub(crate) fn pending_recordings_for_reconnect(
        &self,
    ) -> Result<Vec<PendingRecordingTransfer>, RecordingTransferStoreError> {
        self.pending_recordings_for_reconnect_at(system_epoch_ms())
    }

    pub(crate) fn pending_recordings_for_reconnect_at(
        &self,
        now_ms: u64,
    ) -> Result<Vec<PendingRecordingTransfer>, RecordingTransferStoreError> {
        let recovered = self
            .disk
            .cleanup_expired(
                now_ms,
                self.config.writing_expiry,
                self.config.finalized_expiry,
            )
            .map_err(RecordingTransferStoreError::Disk)?;
        pending_recordings_for_reconnect(&recovered).map_err(RecordingTransferStoreError::Recovery)
    }

    #[cfg(any(windows, test))]
    pub(crate) fn recording_chunk_at(
        &self,
        recording_id: &str,
        chunk_index: u32,
        now_ms: u64,
    ) -> Result<Option<EncodedChunk>, RecordingTransferStoreError> {
        let Some(mut record) = self.pending_record(recording_id, now_ms)? else {
            return Ok(None);
        };
        let total_bytes = record
            .total_bytes
            .ok_or(RecordingTransferStoreError::InvalidState(
                TempStoreError::InvalidSize,
            ))?;
        record
            .validate_chunk_request(u64::from(chunk_index))
            .map_err(RecordingTransferStoreError::Transfer)?;
        let path = self
            .disk
            .recording_media_path(recording_id)
            .map_err(RecordingTransferStoreError::Disk)?;
        let chunk = match &record.media_protection {
            Some(protection) => read_protected_media_chunk(
                MediaTransferKind::Recording,
                &path,
                protection,
                total_bytes,
                u64::from(chunk_index),
                RECORDING_CHUNK_ENVELOPE_UTF8_BYTES,
            ),
            None => Err(TransferError::ProtectedMedia),
        };
        let chunk = chunk.map_err(RecordingTransferStoreError::Transfer)?;
        record
            .record_chunk_read(u64::from(chunk_index), now_ms)
            .map_err(RecordingTransferStoreError::InvalidState)?;
        self.disk
            .write_record(&record)
            .map_err(RecordingTransferStoreError::Disk)?;
        Ok(Some(chunk))
    }

    pub(crate) fn ack_recording(
        &self,
        recording_id: &str,
    ) -> Result<bool, RecordingTransferStoreError> {
        self.ack_recording_at(recording_id, system_epoch_ms())
    }

    pub(crate) fn ack_recording_at(
        &self,
        recording_id: &str,
        now_ms: u64,
    ) -> Result<bool, RecordingTransferStoreError> {
        let Some(mut record) = self.pending_record(recording_id, now_ms)? else {
            return Ok(false);
        };
        record
            .ack(now_ms)
            .map_err(RecordingTransferStoreError::InvalidState)?;
        self.delete_recording(recording_id)?;
        Ok(true)
    }

    pub(crate) fn reject_recording(
        &self,
        recording_id: &str,
        retain_for_diagnostics: bool,
    ) -> Result<bool, RecordingTransferStoreError> {
        self.reject_recording_at(recording_id, retain_for_diagnostics, system_epoch_ms())
    }

    pub(crate) fn reject_recording_at(
        &self,
        recording_id: &str,
        retain_for_diagnostics: bool,
        now_ms: u64,
    ) -> Result<bool, RecordingTransferStoreError> {
        let Some(mut record) = self.pending_record(recording_id, now_ms)? else {
            return Ok(false);
        };
        if retain_for_diagnostics {
            record
                .reject(
                    now_ms,
                    Some(now_ms.saturating_add(self.config.rejected_retention)),
                )
                .map_err(RecordingTransferStoreError::InvalidState)?;
            self.disk
                .write_record(&record)
                .map_err(RecordingTransferStoreError::Disk)?;
        } else {
            record
                .reject(now_ms, None)
                .map_err(RecordingTransferStoreError::InvalidState)?;
            self.delete_recording(recording_id)?;
        }
        Ok(true)
    }

    fn pending_record(
        &self,
        recording_id: &str,
        now_ms: u64,
    ) -> Result<Option<TempMediaRecord>, RecordingTransferStoreError> {
        let Some(record) = self
            .disk
            .read_record(recording_id)
            .map_err(RecordingTransferStoreError::Disk)?
        else {
            return Ok(None);
        };
        let decision = record
            .recovery_decision(
                now_ms,
                self.config.writing_expiry,
                self.config.finalized_expiry,
            )
            .map_err(RecordingTransferStoreError::InvalidState)?;
        match cleanup_recovery_decision(decision, record.is_writing(), false) {
            StartupRecoveryDecision::ReportPending => Ok(Some(record)),
            StartupRecoveryDecision::Delete => {
                self.delete_recording(recording_id)?;
                Ok(None)
            }
            StartupRecoveryDecision::KeepWriting | StartupRecoveryDecision::RetainRejected => {
                Ok(None)
            }
        }
    }

    fn delete_recording(&self, recording_id: &str) -> Result<(), RecordingTransferStoreError> {
        self.disk
            .delete_recording_media(recording_id)
            .map_err(RecordingTransferStoreError::Disk)?;
        self.disk
            .delete_record(recording_id)
            .map_err(RecordingTransferStoreError::Disk)
    }
}

fn system_epoch_ms() -> u64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => u64::try_from(duration.as_millis()).unwrap_or(u64::MAX),
        Err(_) => 0,
    }
}
