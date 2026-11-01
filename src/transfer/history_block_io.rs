//! Shared authenticated block/index IO; owners commit their heads only after both files sync.

use super::{
    hashing::sha256_hex,
    trajectory_crypto::TrajectoryCipher,
    trajectory_index::{INDEX_ENTRY_BYTES, TrajectoryIndexEntry},
    trajectory_ledger::TrajectoryStoreError,
};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
};

pub(super) fn read_block(
    data: &mut File,
    cipher: &TrajectoryCipher,
    entry: &TrajectoryIndexEntry,
) -> Result<Vec<u8>, TrajectoryStoreError> {
    let length =
        usize::try_from(entry.manifest.byte_length).map_err(|_| TrajectoryStoreError::Invalid)?;
    data.seek(SeekFrom::Start(entry.data_offset))
        .map_err(|_| TrajectoryStoreError::Io)?;
    let mut header = [0; 36];
    data.read_exact(&mut header)
        .map_err(|_| TrajectoryStoreError::Io)?;
    if header[..4] != entry.manifest.byte_length.to_le_bytes()
        || header[4..12] != entry.manifest.sequence.to_le_bytes()
    {
        return Err(TrajectoryStoreError::Invalid);
    }
    let nonce: [u8; 24] = header[12..]
        .try_into()
        .map_err(|_| TrajectoryStoreError::Invalid)?;
    let mut ciphertext = vec![0; length + 16];
    data.read_exact(&mut ciphertext)
        .map_err(|_| TrajectoryStoreError::Io)?;
    let plaintext = cipher
        .unseal(entry.manifest.sequence, length, &nonce, &ciphertext)
        .map_err(|_| TrajectoryStoreError::Authentication)?;
    if sha256_hex(&plaintext) != entry.manifest.block_sha256 {
        return Err(TrajectoryStoreError::Authentication);
    }
    Ok(plaintext)
}

pub(super) fn read_entry(
    index: &mut File,
    cipher: &TrajectoryCipher,
    sequence: u64,
    committed_blocks: u64,
    committed_data_bytes: u64,
) -> Result<TrajectoryIndexEntry, TrajectoryStoreError> {
    if sequence >= committed_blocks {
        return Err(TrajectoryStoreError::Invalid);
    }
    let offset = sequence
        .checked_mul(INDEX_ENTRY_BYTES as u64)
        .ok_or(TrajectoryStoreError::Invalid)?;
    index
        .seek(SeekFrom::Start(offset))
        .map_err(|_| TrajectoryStoreError::Io)?;
    let mut bytes = [0; INDEX_ENTRY_BYTES];
    index
        .read_exact(&mut bytes)
        .map_err(|_| TrajectoryStoreError::Io)?;
    let entry = TrajectoryIndexEntry::decode(&bytes, cipher)
        .map_err(|_| TrajectoryStoreError::Authentication)?;
    if entry.manifest.sequence != sequence
        || entry
            .data_offset
            .checked_add(52 + u64::from(entry.manifest.byte_length))
            .is_none_or(|end| end > committed_data_bytes)
    {
        return Err(TrajectoryStoreError::Invalid);
    }
    Ok(entry)
}

pub(super) fn commit_block(
    data: &mut File,
    index: &mut File,
    cipher: &TrajectoryCipher,
    bytes: &[u8],
    entry: &TrajectoryIndexEntry,
) -> Result<(), TrajectoryStoreError> {
    let sealed = cipher
        .seal(entry.manifest.sequence, bytes)
        .map_err(|_| TrajectoryStoreError::Authentication)?;
    let encoded = entry
        .encode(cipher)
        .map_err(|_| TrajectoryStoreError::Invalid)?;
    data.seek(SeekFrom::Start(entry.data_offset))
        .map_err(|_| TrajectoryStoreError::Io)?;
    data.write_all(&entry.manifest.byte_length.to_le_bytes())
        .and_then(|()| data.write_all(&entry.manifest.sequence.to_le_bytes()))
        .and_then(|()| data.write_all(&sealed.nonce))
        .and_then(|()| data.write_all(&sealed.ciphertext))
        .and_then(|()| data.sync_all())
        .map_err(|_| TrajectoryStoreError::Io)?;
    index
        .seek(SeekFrom::Start(
            entry
                .manifest
                .sequence
                .checked_mul(INDEX_ENTRY_BYTES as u64)
                .ok_or(TrajectoryStoreError::Invalid)?,
        ))
        .map_err(|_| TrajectoryStoreError::Io)?;
    index
        .write_all(&encoded)
        .and_then(|()| index.sync_all())
        .map_err(|_| TrajectoryStoreError::Io)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct HistoryChunk {
    pub(crate) sequence: u64,
    pub(crate) block_bytes: u32,
    pub(crate) block_sha256: String,
    pub(crate) chunk_index: u8,
    pub(crate) chunk_count: u8,
    pub(crate) chunk_byte_offset: u32,
    pub(crate) chunk_raw_bytes: u32,
    pub(crate) chunk_sha256: String,
    pub(crate) base64: String,
}

pub(super) fn chunk(
    data: &mut File,
    cipher: &TrajectoryCipher,
    entry: &TrajectoryIndexEntry,
    index: u8,
) -> Result<HistoryChunk, TrajectoryStoreError> {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    let count = u8::try_from(entry.manifest.byte_length.div_ceil(32768))
        .map_err(|_| TrajectoryStoreError::Invalid)?;
    if index >= count {
        return Err(TrajectoryStoreError::Invalid);
    }
    let bytes = read_block(data, cipher, entry)?;
    let offset = usize::from(index) * 32768;
    let raw = &bytes[offset..bytes.len().min(offset + 32768)];
    Ok(HistoryChunk {
        sequence: entry.manifest.sequence,
        block_bytes: entry.manifest.byte_length,
        block_sha256: entry.manifest.block_sha256.clone(),
        chunk_index: index,
        chunk_count: count,
        chunk_byte_offset: u32::try_from(offset).map_err(|_| TrajectoryStoreError::Invalid)?,
        chunk_raw_bytes: u32::try_from(raw.len()).map_err(|_| TrajectoryStoreError::Invalid)?,
        chunk_sha256: sha256_hex(raw),
        base64: STANDARD.encode(raw),
    })
}
