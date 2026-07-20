//! Protected-at-rest screenshot media helpers.

use std::path::Path;

use crate::transfer::{
    protected_file::{PROTECTED_MEDIA_ALGORITHM, protect_file_in_place},
    protected_key::{ProtectedMediaKey, new_protected_media_key, unprotect_media_key},
};

use super::ScreenshotTransferStoreError;

pub(super) fn protect_screenshot_media_file(
    path: &Path,
) -> Result<(u64, String, ProtectedMediaKey), ScreenshotTransferStoreError> {
    let (protection, key) = new_protected_media_key(PROTECTED_MEDIA_ALGORITHM)
        .map_err(|_| ScreenshotTransferStoreError::Protection)?;
    let (_, nonce_prefix) = unprotect_media_key(&protection, PROTECTED_MEDIA_ALGORITHM)
        .map_err(|_| ScreenshotTransferStoreError::Protection)?;
    let info = protect_file_in_place(path, &key, nonce_prefix)
        .map_err(|_| ScreenshotTransferStoreError::Protection)?;
    Ok((info.plaintext_bytes, info.plaintext_sha256, protection))
}
