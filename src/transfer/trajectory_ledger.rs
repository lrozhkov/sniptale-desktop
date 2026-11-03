//! Bounded authenticated commit head, independent of the video's durable receipt.

use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

use super::{protected_key::ProtectedMediaKey, trajectory_crypto::TrajectoryCipher};
use crate::protocol::{
    DeliveryRoute,
    trajectory::{
        CursorTrajectoryDescriptor, MAX_SAFE_INTEGER, valid_trajectory_hash, valid_trajectory_id,
    },
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TrajectoryStoreError {
    Io,
    Invalid,
    Authentication,
    Busy,
    Retired,
    Conflict,
}

impl std::fmt::Display for TrajectoryStoreError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "trajectory store: {self:?}")
    }
}

impl std::error::Error for TrajectoryStoreError {}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct TrajectoryVideoBinding {
    pub(crate) sha256: String,
    pub(crate) bytes: u64,
    pub(crate) duration_ms: u64,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct TrajectoryLedger {
    pub(super) version: u16,
    pub(super) recording_id: String,
    pub(super) trajectory_id: String,
    pub(super) writer_generation: String,
    pub(super) key: ProtectedMediaKey,
    pub(super) route: DeliveryRoute,
    pub(super) video: Option<TrajectoryVideoBinding>,
    pub(super) video_receipt_accepted: bool,
    pub(super) attachment_receipt_accepted: bool,
    pub(super) retired: bool,
    pub(super) committed_data_bytes: u64,
    pub(super) descriptor: CursorTrajectoryDescriptor,
}

impl TrajectoryLedger {
    pub(super) fn read(path: &Path) -> Result<(Self, TrajectoryCipher), TrajectoryStoreError> {
        super::history_head::read(path, |ledger: &Self| {
            ledger.validate()?;
            TrajectoryCipher::open(&ledger.recording_id, &ledger.trajectory_id, &ledger.key)
                .map_err(|_| TrajectoryStoreError::Authentication)
        })
    }

    pub(super) fn write(
        &self,
        path: &Path,
        cipher: &TrajectoryCipher,
    ) -> Result<(), TrajectoryStoreError> {
        self.validate()?;
        super::history_head::write(self, path, cipher)
    }

    fn validate(&self) -> Result<(), TrajectoryStoreError> {
        crate::protocol::trajectory::validate_trajectory_descriptor(&self.descriptor)
            .map_err(|_| TrajectoryStoreError::Invalid)?;
        if self.version != 1
            || !valid_trajectory_id(&self.trajectory_id)
            || !valid_trajectory_id(&self.writer_generation)
            || self.recording_id.is_empty()
            || self.recording_id.len() > 128
            || !self.recording_id.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-')
            })
            || !self.route.valid_local_route()
            || self.descriptor.trajectory_id != self.trajectory_id
            || self.descriptor.format_version != 1
            || !valid_trajectory_hash(&self.descriptor.stream_sha256)
            || [
                self.descriptor.total_blocks,
                self.descriptor.total_records,
                self.descriptor.total_samples,
                self.descriptor.total_bytes,
            ]
            .into_iter()
            .any(|value| value > MAX_SAFE_INTEGER)
            || self.descriptor.total_samples > self.descriptor.total_records
            || self.video.as_ref().is_some_and(|video| {
                !valid_trajectory_hash(&video.sha256)
                    || video.bytes == 0
                    || video.bytes > MAX_SAFE_INTEGER
                    || video.duration_ms > MAX_SAFE_INTEGER
                    || video.width == 0
                    || video.height == 0
            })
            || (self.video_receipt_accepted && self.video.is_none())
            || (self.attachment_receipt_accepted && !self.descriptor.finalized)
        {
            return Err(TrajectoryStoreError::Invalid);
        }
        Ok(())
    }
}

pub(super) fn check_private_path(
    path: &Path,
    allow_missing: bool,
) -> Result<(), TrajectoryStoreError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if allow_missing && error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(());
        }
        Err(_) => return Err(TrajectoryStoreError::Io),
    };
    if metadata.file_type().is_symlink() {
        return Err(TrajectoryStoreError::Invalid);
    }
    #[cfg(windows)]
    {
        if super::windows_files::is_reparse_point(&metadata) {
            return Err(TrajectoryStoreError::Invalid);
        }
    }
    Ok(())
}
