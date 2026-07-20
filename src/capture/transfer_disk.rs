//! Disk-backed screenshot transfer metadata and media files.

use std::{
    fs::{self, File},
    io::BufReader,
    path::{Path, PathBuf},
};

use super::transfer_record::{ScreenshotTransferRecord, validate_capture_id};
use super::transfer_store::ScreenshotTransferStoreError;
use crate::transfer::identity::{TransferMediaFileKind, media_path};

const METADATA_EXTENSION: &str = "json";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ScreenshotTransferDiskStore {
    root: PathBuf,
}

impl ScreenshotTransferDiskStore {
    pub(super) fn open(root: impl Into<PathBuf>) -> Result<Self, ScreenshotTransferStoreError> {
        let root = root.into();
        fs::create_dir_all(&root).map_err(|_| ScreenshotTransferStoreError::Io)?;
        Ok(Self { root })
    }

    #[cfg(any(windows, test))]
    pub(super) fn read_record(
        &self,
        capture_id: &str,
    ) -> Result<Option<ScreenshotTransferRecord>, ScreenshotTransferStoreError> {
        validate_capture_id(capture_id)?;
        let path = self.metadata_path(capture_id)?;
        if !path.exists() {
            return Ok(None);
        }
        read_record(&path).map(Some)
    }

    pub(super) fn scan_records(
        &self,
    ) -> Result<Vec<ScreenshotTransferRecord>, ScreenshotTransferStoreError> {
        let mut records = Vec::new();
        for entry in fs::read_dir(&self.root).map_err(|_| ScreenshotTransferStoreError::Io)? {
            let path = entry.map_err(|_| ScreenshotTransferStoreError::Io)?.path();
            if is_metadata_path(&path) {
                records.push(read_record(&path)?);
            }
        }
        records.sort_by(|left, right| left.capture_id.cmp(&right.capture_id));
        Ok(records)
    }

    #[cfg(any(windows, test))]
    pub(super) fn write_record(
        &self,
        record: &ScreenshotTransferRecord,
    ) -> Result<(), ScreenshotTransferStoreError> {
        record.validate()?;
        let temp_path = self.temp_metadata_path(&record.capture_id)?;
        let final_path = self.metadata_path(&record.capture_id)?;
        {
            let file = File::create(&temp_path).map_err(|_| ScreenshotTransferStoreError::Io)?;
            serde_json::to_writer(file, record)
                .map_err(|_| ScreenshotTransferStoreError::InvalidMetadata)?;
        }
        fs::rename(temp_path, final_path).map_err(|_| ScreenshotTransferStoreError::Io)
    }

    pub(super) fn screenshot_media_path(
        &self,
        capture_id: &str,
    ) -> Result<PathBuf, ScreenshotTransferStoreError> {
        validate_capture_id(capture_id)?;
        media_path(&self.root, capture_id, TransferMediaFileKind::Png)
            .map_err(|_| ScreenshotTransferStoreError::InvalidMetadata)
    }

    pub(super) fn delete_capture(
        &self,
        capture_id: &str,
    ) -> Result<(), ScreenshotTransferStoreError> {
        self.delete_media(capture_id)?;
        self.delete_record(capture_id)
    }

    fn delete_media(&self, capture_id: &str) -> Result<(), ScreenshotTransferStoreError> {
        let path = self.screenshot_media_path(capture_id)?;
        remove_optional_file(path)
    }

    fn delete_record(&self, capture_id: &str) -> Result<(), ScreenshotTransferStoreError> {
        validate_capture_id(capture_id)?;
        remove_optional_file(self.metadata_path(capture_id)?)
    }

    fn metadata_path(&self, capture_id: &str) -> Result<PathBuf, ScreenshotTransferStoreError> {
        debug_assert_eq!(METADATA_EXTENSION, "json");
        media_path(&self.root, capture_id, TransferMediaFileKind::Json)
            .map_err(|_| ScreenshotTransferStoreError::InvalidMetadata)
    }

    #[cfg(any(windows, test))]
    fn temp_metadata_path(
        &self,
        capture_id: &str,
    ) -> Result<PathBuf, ScreenshotTransferStoreError> {
        debug_assert_eq!(METADATA_EXTENSION, "json");
        media_path(&self.root, capture_id, TransferMediaFileKind::JsonTemp)
            .map_err(|_| ScreenshotTransferStoreError::InvalidMetadata)
    }
}

fn read_record(path: &Path) -> Result<ScreenshotTransferRecord, ScreenshotTransferStoreError> {
    let file = File::open(path).map_err(|_| ScreenshotTransferStoreError::Io)?;
    let record: ScreenshotTransferRecord = serde_json::from_reader(BufReader::new(file))
        .map_err(|_| ScreenshotTransferStoreError::InvalidMetadata)?;
    record.validate()?;
    Ok(record)
}

fn remove_optional_file(path: PathBuf) -> Result<(), ScreenshotTransferStoreError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(ScreenshotTransferStoreError::Io),
    }
}

fn is_metadata_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension == METADATA_EXTENSION)
}
