//! Protected media chunk read API shared by screenshot and recording stores.

use std::path::{Path, PathBuf};

use super::{
    chunks::{EncodedChunk, MediaTransferKind, chunk_count},
    error::TransferError,
    protected_file::{PROTECTED_MEDIA_ALGORITHM, encode_protected_file_chunk},
    protected_key::{ProtectedMediaKey, unprotect_media_key},
};

#[derive(Clone, Debug)]
pub(crate) struct ProtectedMediaReader {
    path: PathBuf,
    key: Vec<u8>,
    total_bytes: u64,
    sha256: String,
    next_chunk_index: u64,
    pending_plaintext: Vec<u8>,
    pending_offset: usize,
}

impl ProtectedMediaReader {
    pub(crate) fn open(
        path: PathBuf,
        protection: &ProtectedMediaKey,
        total_bytes: u64,
        sha256: String,
    ) -> Result<Self, TransferError> {
        if total_bytes == 0
            || sha256.len() != 64
            || !sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(TransferError::ProtectedMedia);
        }
        let (key, _) = unprotect_media_key(protection, PROTECTED_MEDIA_ALGORITHM)
            .map_err(|_| TransferError::ProtectedMedia)?;
        Ok(Self {
            path,
            key: key.to_vec(),
            total_bytes,
            sha256,
            next_chunk_index: 0,
            pending_plaintext: Vec::new(),
            pending_offset: 0,
        })
    }

    #[must_use]
    pub(crate) const fn total_bytes(&self) -> u64 {
        self.total_bytes
    }

    #[must_use]
    pub(crate) fn sha256(&self) -> &str {
        &self.sha256
    }

    pub(crate) fn read_next_plaintext(
        &mut self,
        maximum_bytes: usize,
    ) -> Result<Option<Vec<u8>>, TransferError> {
        if maximum_bytes == 0 {
            return Err(TransferError::InvalidLimit);
        }
        if self.pending_offset == self.pending_plaintext.len() {
            self.pending_plaintext.fill(0);
            self.pending_plaintext.clear();
            self.pending_offset = 0;
            let chunks = chunk_count(self.total_bytes)?;
            if self.next_chunk_index == chunks {
                return Ok(None);
            }
            self.pending_plaintext = super::protected_file::read_protected_file_plaintext_chunk(
                &self.path,
                &self.key,
                self.next_chunk_index,
            )
            .map_err(|_| TransferError::ProtectedMedia)?;
            if self.pending_plaintext.is_empty() {
                return Err(TransferError::ProtectedMedia);
            }
            self.next_chunk_index = self
                .next_chunk_index
                .checked_add(1)
                .ok_or(TransferError::InvalidLimit)?;
        }
        let end = self
            .pending_offset
            .checked_add(maximum_bytes)
            .ok_or(TransferError::InvalidLimit)?
            .min(self.pending_plaintext.len());
        let bytes = self
            .pending_plaintext
            .get(self.pending_offset..end)
            .ok_or(TransferError::InvalidLimit)?
            .to_vec();
        self.pending_offset = end;
        if bytes.is_empty() {
            return Err(TransferError::InvalidLimit);
        }
        Ok(Some(bytes))
    }
}

impl Drop for ProtectedMediaReader {
    fn drop(&mut self) {
        self.key.fill(0);
        self.pending_plaintext.fill(0);
    }
}

pub(crate) fn read_protected_media_chunk(
    kind: MediaTransferKind,
    path: &Path,
    protection: &ProtectedMediaKey,
    total_bytes: u64,
    chunk_index: u64,
    envelope_utf8_bytes: usize,
) -> Result<EncodedChunk, TransferError> {
    let chunks = chunk_count(total_bytes)?;
    if chunk_index >= chunks {
        return Err(TransferError::ChunkIndexOutOfBounds {
            index: chunk_index,
            chunk_count: chunks,
        });
    }
    let (key, _) = unprotect_media_key(protection, PROTECTED_MEDIA_ALGORITHM)
        .map_err(|_| TransferError::ProtectedMedia)?;
    encode_protected_file_chunk(
        kind,
        path,
        &key,
        total_bytes,
        chunk_index,
        envelope_utf8_bytes,
    )
}
