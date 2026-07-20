//! Screenshot transfer record validation helpers.

use super::ScreenshotTransferRecordError;
use crate::transfer::identity::{
    TransferMediaFileKind, validate_media_filename, validate_media_id, validate_sha256_hex,
};

pub(in crate::capture) fn validate_capture_id(
    capture_id: &str,
) -> Result<(), ScreenshotTransferRecordError> {
    validate_media_id(capture_id).map_err(|_| ScreenshotTransferRecordError::InvalidId)
}

pub(super) fn validate_filename(filename: &str) -> Result<(), ScreenshotTransferRecordError> {
    validate_media_filename(filename, TransferMediaFileKind::Png)
        .map_err(|_| ScreenshotTransferRecordError::InvalidFilename)
}

pub(super) fn validate_sha256(sha256: &str) -> Result<(), ScreenshotTransferRecordError> {
    validate_sha256_hex(sha256).map_err(|_| ScreenshotTransferRecordError::InvalidState)
}
