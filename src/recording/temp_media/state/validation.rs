//! Temp-media state validation helpers.

use super::TempStoreError;
use crate::transfer::identity::{
    TransferIdentityError, validate_media_id as validate_transfer_media_id, validate_sha256_hex,
};

pub(super) fn validate_media_id(media_id: &str) -> Result<(), TempStoreError> {
    match validate_transfer_media_id(media_id) {
        Ok(()) => Ok(()),
        Err(TransferIdentityError::EmptyId) => Err(TempStoreError::EmptyMediaId),
        Err(_) => Err(TempStoreError::InvalidMediaId),
    }
}

pub(super) fn validate_size(total_bytes: u64) -> Result<(), TempStoreError> {
    if total_bytes == 0 {
        Err(TempStoreError::InvalidSize)
    } else {
        Ok(())
    }
}

pub(super) fn validate_sha256(sha256: &str) -> Result<(), TempStoreError> {
    validate_sha256_hex(sha256).map_err(|_| TempStoreError::InvalidSha256)
}
