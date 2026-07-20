//! File-backed media chunk reader for transfer workers.

use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
};

use super::{
    chunks::{EncodedChunk, MediaTransferKind, chunk_count, encode_chunk},
    error::TransferError,
};
use crate::protocol::CHUNK_RAW_BYTES;

pub(crate) fn encode_file_chunk(
    kind: MediaTransferKind,
    path: &Path,
    total_bytes: u64,
    index: u64,
    envelope_utf8_bytes: usize,
) -> Result<EncodedChunk, TransferError> {
    let count = chunk_count(total_bytes)?;
    if index >= count {
        return Err(TransferError::ChunkIndexOutOfBounds {
            index,
            chunk_count: count,
        });
    }
    let byte_offset = index
        .checked_mul(CHUNK_RAW_BYTES)
        .ok_or(TransferError::InvalidLimit)?;
    let expected_raw_bytes = total_bytes
        .checked_sub(byte_offset)
        .ok_or(TransferError::InvalidLimit)?
        .min(CHUNK_RAW_BYTES);
    let expected = usize::try_from(expected_raw_bytes).map_err(|_| TransferError::InvalidLimit)?;

    let mut file = File::open(path).map_err(|_| TransferError::FileIo)?;
    file.seek(SeekFrom::Start(byte_offset))
        .map_err(|_| TransferError::FileIo)?;
    let mut raw = vec![0_u8; expected];
    file.read_exact(&mut raw)
        .map_err(|_| TransferError::FileIo)?;
    encode_chunk(kind, total_bytes, index, &raw, envelope_utf8_bytes)
}

#[cfg(test)]
pub(crate) fn sha256_file_hex(path: &Path) -> Result<String, TransferError> {
    let mut file = File::open(path).map_err(|_| TransferError::FileIo)?;
    let mut hasher = super::hashing::Sha256Accumulator::new();
    let mut buffer = vec![0_u8; 64 * 1_024].into_boxed_slice();
    loop {
        let read = file.read(&mut buffer).map_err(|_| TransferError::FileIo)?;
        if read == 0 {
            return Ok(hasher.finalize_hex());
        }
        hasher.update(&buffer[..read]);
    }
}
