//! Bounded base64 media chunk owner.

use base64::{Engine as _, engine::general_purpose::STANDARD};

use super::{error::TransferError, hashing::sha256_hex};
#[cfg(any(windows, test))]
use crate::protocol::RECORDING_MAX_TOTAL_BYTES;
#[cfg(any(windows, test))]
use crate::protocol::SCREENSHOT_MAX_TOTAL_BYTES;
use crate::protocol::{
    CHUNK_MAX_BASE64_CHARS, CHUNK_RAW_BYTES, HOST_TO_EXTENSION_MAX_MESSAGE_BYTES,
    MESSAGE_TARGET_MAX_JSON_UTF8_BYTES,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MediaTransferKind {
    #[cfg(any(windows, test))]
    Screenshot,
    #[cfg(any(windows, test))]
    Recording,
}

impl MediaTransferKind {
    const fn max_total_bytes(self) -> u64 {
        match self {
            #[cfg(any(windows, test))]
            Self::Screenshot => SCREENSHOT_MAX_TOTAL_BYTES,
            #[cfg(any(windows, test))]
            Self::Recording => RECORDING_MAX_TOTAL_BYTES,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct EncodedChunk {
    pub(crate) index: u64,
    pub(crate) byte_offset: u64,
    pub(crate) raw_bytes: u64,
    pub(crate) chunk_sha256: String,
    pub(crate) base64: String,
}

pub(crate) fn raw_chunk_limit() -> Result<usize, TransferError> {
    usize::try_from(CHUNK_RAW_BYTES).map_err(|_| TransferError::InvalidLimit)
}

pub(crate) fn chunk_count(total_bytes: u64) -> Result<u64, TransferError> {
    if total_bytes == 0 {
        return Err(TransferError::EmptyTransfer);
    }
    Ok(total_bytes.div_ceil(CHUNK_RAW_BYTES))
}

pub(crate) fn encode_chunk(
    kind: MediaTransferKind,
    total_bytes: u64,
    index: u64,
    raw: &[u8],
    envelope_utf8_bytes: usize,
) -> Result<EncodedChunk, TransferError> {
    validate_total_bytes(kind, total_bytes)?;
    let chunk_count = chunk_count(total_bytes)?;
    if index >= chunk_count {
        return Err(TransferError::ChunkIndexOutOfBounds { index, chunk_count });
    }

    let raw_limit = raw_chunk_limit()?;
    if raw.len() > raw_limit {
        return Err(TransferError::ChunkRawBytesTooLarge {
            raw_bytes: raw.len(),
            limit: raw_limit,
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
    if raw.len() != expected {
        return Err(TransferError::ChunkSizeMismatch {
            expected,
            actual: raw.len(),
        });
    }

    let encoded_len = encoded_base64_len(raw.len())?;
    validate_base64_len(encoded_len)?;
    validate_json_message_len(encoded_len, envelope_utf8_bytes)?;

    Ok(EncodedChunk {
        index,
        byte_offset,
        raw_bytes: u64::try_from(raw.len()).map_err(|_| TransferError::InvalidLimit)?,
        chunk_sha256: sha256_hex(raw),
        base64: STANDARD.encode(raw),
    })
}

pub(crate) fn validate_base64_len(encoded_chars: usize) -> Result<(), TransferError> {
    if encoded_chars > CHUNK_MAX_BASE64_CHARS {
        Err(TransferError::Base64TooLarge {
            encoded_chars,
            limit: CHUNK_MAX_BASE64_CHARS,
        })
    } else {
        Ok(())
    }
}

fn validate_total_bytes(kind: MediaTransferKind, total_bytes: u64) -> Result<(), TransferError> {
    if total_bytes == 0 {
        return Err(TransferError::EmptyTransfer);
    }
    let limit = kind.max_total_bytes();
    if total_bytes > limit {
        Err(TransferError::TotalBytesTooLarge { total_bytes, limit })
    } else {
        Ok(())
    }
}

fn encoded_base64_len(raw_bytes: usize) -> Result<usize, TransferError> {
    raw_bytes
        .div_ceil(3)
        .checked_mul(4)
        .ok_or(TransferError::Base64TooLarge {
            encoded_chars: usize::MAX,
            limit: CHUNK_MAX_BASE64_CHARS,
        })
}

fn validate_json_message_len(
    encoded_chars: usize,
    envelope_utf8_bytes: usize,
) -> Result<(), TransferError> {
    let limit = MESSAGE_TARGET_MAX_JSON_UTF8_BYTES.min(HOST_TO_EXTENSION_MAX_MESSAGE_BYTES);
    let json_bytes = encoded_chars.checked_add(envelope_utf8_bytes).ok_or(
        TransferError::JsonMessageTooLarge {
            json_bytes: usize::MAX,
            limit,
        },
    )?;
    if json_bytes > limit {
        Err(TransferError::JsonMessageTooLarge { json_bytes, limit })
    } else {
        Ok(())
    }
}
