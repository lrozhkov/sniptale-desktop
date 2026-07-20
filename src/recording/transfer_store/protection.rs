//! Protected-at-rest recording media helpers.

use std::path::Path;

use crate::transfer::{
    protected_file::{PROTECTED_MEDIA_ALGORITHM, protect_file_in_place},
    protected_key::{ProtectedMediaKey, new_protected_media_key, unprotect_media_key},
};

use super::RecordingTransferStoreError;

pub(super) fn protect_recording_media(
    path: &Path,
) -> Result<(u64, String, ProtectedMediaKey), RecordingTransferStoreError> {
    let (protection, key) = new_protected_media_key(PROTECTED_MEDIA_ALGORITHM)
        .map_err(|_| RecordingTransferStoreError::Protection)?;
    let (_, nonce_prefix) = unprotect_media_key(&protection, PROTECTED_MEDIA_ALGORITHM)
        .map_err(|_| RecordingTransferStoreError::Protection)?;
    let info = protect_file_in_place(path, &key, nonce_prefix)
        .map_err(|_| RecordingTransferStoreError::Protection)?;
    Ok((info.plaintext_bytes, info.plaintext_sha256, protection))
}
