//! Disk-backed temporary media metadata store.
use std::{
    fs::{self, File},
    io::BufReader,
    path::{Path, PathBuf},
};

use super::{StartupRecoveryDecision, TempMediaRecord, TempStoreError};
use crate::transfer::{
    identity::{TransferMediaFileKind, media_path},
    lifecycle::cleanup_recovery_decision,
};

const METADATA_EXTENSION: &str = "json";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TempStoreDiskError {
    Io,
    InvalidMetadata,
    InvalidMedia(TempStoreError),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RecoveredTempMedia {
    pub(crate) record: TempMediaRecord,
    pub(crate) decision: StartupRecoveryDecision,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TempMediaDiskStore {
    root: PathBuf,
}

impl TempMediaDiskStore {
    pub(crate) fn open(root: impl Into<PathBuf>) -> Result<Self, TempStoreDiskError> {
        let root = root.into();
        fs::create_dir_all(&root).map_err(|_| TempStoreDiskError::Io)?;
        Ok(Self { root })
    }

    pub(crate) fn write_record(&self, record: &TempMediaRecord) -> Result<(), TempStoreDiskError> {
        validate_record_for_disk(record)?;
        let final_path = self.metadata_path(&record.media_id)?;
        let temp_path = self.temp_metadata_path(&record.media_id)?;
        {
            let file = File::create(&temp_path).map_err(|_| TempStoreDiskError::Io)?;
            serde_json::to_writer(file, record).map_err(|_| TempStoreDiskError::InvalidMetadata)?;
        }
        fs::rename(temp_path, final_path).map_err(|_| TempStoreDiskError::Io)
    }

    pub(crate) fn read_record(
        &self,
        media_id: &str,
    ) -> Result<Option<TempMediaRecord>, TempStoreDiskError> {
        TempMediaRecord::validate_media_id(media_id).map_err(TempStoreDiskError::InvalidMedia)?;
        let path = self.metadata_path(media_id)?;
        if !path.exists() {
            return Ok(None);
        }
        let record = read_record(&path)?;
        validate_record_for_disk(&record)?;
        Ok(Some(record))
    }

    pub(crate) fn scan_recovery(
        &self,
        now_ms: u64,
        incomplete_ttl_ms: u64,
        finalized_ttl_ms: u64,
    ) -> Result<Vec<RecoveredTempMedia>, TempStoreDiskError> {
        let mut recovered = Vec::new();
        for entry in fs::read_dir(&self.root).map_err(|_| TempStoreDiskError::Io)? {
            let path = entry.map_err(|_| TempStoreDiskError::Io)?.path();
            if !is_metadata_path(&path) {
                continue;
            }
            let record = read_record(&path)?;
            validate_record_for_disk(&record)?;
            let decision = record
                .recovery_decision(now_ms, incomplete_ttl_ms, finalized_ttl_ms)
                .map_err(TempStoreDiskError::InvalidMedia)?;
            recovered.push(RecoveredTempMedia { record, decision });
        }
        recovered.sort_by(|left, right| left.record.media_id.cmp(&right.record.media_id));
        Ok(recovered)
    }

    pub(crate) fn cleanup_expired(
        &self,
        now_ms: u64,
        incomplete_ttl_ms: u64,
        finalized_ttl_ms: u64,
    ) -> Result<Vec<RecoveredTempMedia>, TempStoreDiskError> {
        let recovered = self.scan_recovery(now_ms, incomplete_ttl_ms, finalized_ttl_ms)?;
        let mut kept = Vec::new();
        for item in recovered {
            if cleanup_recovery_decision(item.decision, item.record.is_writing(), false)
                == StartupRecoveryDecision::Delete
            {
                self.delete_recording_media(&item.record.media_id)?;
                self.delete_record(&item.record.media_id)?;
            } else {
                kept.push(item);
            }
        }
        Ok(kept)
    }

    pub(crate) fn delete_record(&self, media_id: &str) -> Result<(), TempStoreDiskError> {
        TempMediaRecord::validate_media_id(media_id).map_err(TempStoreDiskError::InvalidMedia)?;
        let path = self.metadata_path(media_id)?;
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(TempStoreDiskError::Io),
        }
    }

    pub(crate) fn recording_media_path(
        &self,
        media_id: &str,
    ) -> Result<PathBuf, TempStoreDiskError> {
        TempMediaRecord::validate_media_id(media_id).map_err(TempStoreDiskError::InvalidMedia)?;
        media_path(&self.root, media_id, TransferMediaFileKind::Mp4)
            .map_err(|_| TempStoreDiskError::InvalidMetadata)
    }

    pub(crate) fn delete_recording_media(&self, media_id: &str) -> Result<(), TempStoreDiskError> {
        let path = self.recording_media_path(media_id)?;
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(TempStoreDiskError::Io),
        }
    }

    fn metadata_path(&self, media_id: &str) -> Result<PathBuf, TempStoreDiskError> {
        debug_assert_eq!(METADATA_EXTENSION, "json");
        media_path(&self.root, media_id, TransferMediaFileKind::Json)
            .map_err(|_| TempStoreDiskError::InvalidMetadata)
    }

    fn temp_metadata_path(&self, media_id: &str) -> Result<PathBuf, TempStoreDiskError> {
        debug_assert_eq!(METADATA_EXTENSION, "json");
        media_path(&self.root, media_id, TransferMediaFileKind::JsonTemp)
            .map_err(|_| TempStoreDiskError::InvalidMetadata)
    }
}

fn read_record(path: &Path) -> Result<TempMediaRecord, TempStoreDiskError> {
    let file = File::open(path).map_err(|_| TempStoreDiskError::Io)?;
    serde_json::from_reader(BufReader::new(file)).map_err(|_| TempStoreDiskError::InvalidMetadata)
}

fn validate_record_for_disk(record: &TempMediaRecord) -> Result<(), TempStoreDiskError> {
    record
        .validate_for_persistence()
        .map_err(TempStoreDiskError::InvalidMedia)
}

fn is_metadata_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension == METADATA_EXTENSION)
}
