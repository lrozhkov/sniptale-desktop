//! Finalized recording metadata validation for reconnect reporting.
use serde::{Deserialize, Serialize};

use super::state::TempStoreError;
use crate::transfer::identity::{
    TransferMediaFileKind, validate_media_filename as validate_transfer_media_filename,
};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct RecordingMediaMetadata {
    pub(crate) filename: String,
    pub(crate) duration_ms: u64,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) fps: u16,
    pub(crate) open_editor: bool,
}

pub(crate) fn validate_recording_metadata(
    metadata: &RecordingMediaMetadata,
) -> Result<(), TempStoreError> {
    validate_recording_filename(&metadata.filename)?;
    if metadata.duration_ms == 0 {
        return Err(TempStoreError::InvalidDuration);
    }
    if metadata.width == 0 || metadata.height == 0 {
        return Err(TempStoreError::InvalidDimensions);
    }
    if metadata.fps == 0 {
        return Err(TempStoreError::InvalidFrameRate);
    }
    Ok(())
}

fn validate_recording_filename(filename: &str) -> Result<(), TempStoreError> {
    validate_transfer_media_filename(filename, TransferMediaFileKind::Mp4)
        .map_err(|_| TempStoreError::InvalidFilename)
}
