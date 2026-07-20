//! Encrypted-at-rest media file format with plaintext-compatible transfer chunks.

use std::{
    fs::{self, File},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead, KeyInit, Payload},
};

use super::{
    chunks::{EncodedChunk, MediaTransferKind, encode_chunk},
    error::TransferError,
    hashing::Sha256Accumulator,
};
use crate::protocol::CHUNK_RAW_BYTES;

const MAGIC: &[u8; 8] = b"SYXMF001";
const HEADER_LEN: u64 = 8 + 8 + 8 + 16;
const NONCE_PREFIX_BYTES: usize = 16;
const KEY_BYTES: usize = 32;
const TAG_BYTES: u64 = 16;
pub(crate) const PROTECTED_MEDIA_ALGORITHM: &str = "xchacha20poly1305-chunk-v1";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ProtectedMediaInfo {
    pub(crate) plaintext_bytes: u64,
    pub(crate) plaintext_sha256: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ProtectedFileError {
    Io,
    EmptyFile,
    InvalidKey,
    InvalidHeader,
    InvalidChunk,
    InvalidLimit,
    Crypto,
}

pub(crate) fn protect_file_in_place(
    path: &Path,
    key: &[u8],
    nonce_prefix: [u8; NONCE_PREFIX_BYTES],
) -> Result<ProtectedMediaInfo, ProtectedFileError> {
    let protected_path = protected_temp_path(path)?;
    let result = protect_file_to(path, &protected_path, key, nonce_prefix);
    if result.is_err() {
        let _ = fs::remove_file(&protected_path);
        return result;
    }
    fs::remove_file(path).map_err(|_| ProtectedFileError::Io)?;
    fs::rename(&protected_path, path).map_err(|_| ProtectedFileError::Io)?;
    result
}

pub(crate) fn encode_protected_file_chunk(
    kind: MediaTransferKind,
    path: &Path,
    key: &[u8],
    total_bytes: u64,
    index: u64,
    envelope_utf8_bytes: usize,
) -> Result<EncodedChunk, TransferError> {
    let header = read_header(path).map_err(|_| TransferError::ProtectedMedia)?;
    if header.plaintext_bytes != total_bytes {
        return Err(TransferError::ProtectedMedia);
    }
    let raw = read_plaintext_chunk(path, key, &header, index)
        .map_err(|_| TransferError::ProtectedMedia)?;
    encode_chunk(kind, total_bytes, index, &raw, envelope_utf8_bytes)
}

fn protect_file_to(
    input_path: &Path,
    output_path: &Path,
    key: &[u8],
    nonce_prefix: [u8; NONCE_PREFIX_BYTES],
) -> Result<ProtectedMediaInfo, ProtectedFileError> {
    let cipher = cipher(key)?;
    let mut input = File::open(input_path).map_err(|_| ProtectedFileError::Io)?;
    let plaintext_bytes = input.metadata().map_err(|_| ProtectedFileError::Io)?.len();
    if plaintext_bytes == 0 {
        return Err(ProtectedFileError::EmptyFile);
    }
    let mut output = File::create(output_path).map_err(|_| ProtectedFileError::Io)?;
    let header = ProtectedFileHeader {
        plaintext_bytes,
        chunk_bytes: CHUNK_RAW_BYTES,
        nonce_prefix,
    };
    write_header(&mut output, &header)?;

    let mut hasher = Sha256Accumulator::new();
    let mut index = 0_u64;
    let chunk_len =
        usize::try_from(CHUNK_RAW_BYTES).map_err(|_| ProtectedFileError::InvalidLimit)?;
    let mut buffer = vec![0_u8; chunk_len];
    loop {
        let read = input
            .read(&mut buffer)
            .map_err(|_| ProtectedFileError::Io)?;
        if read == 0 {
            break;
        }
        let plaintext = &buffer[..read];
        hasher.update(plaintext);
        let ciphertext = cipher
            .encrypt(
                XNonce::from_slice(&nonce(&header, index)),
                Payload {
                    msg: plaintext,
                    aad: &chunk_aad(&header, index),
                },
            )
            .map_err(|_| ProtectedFileError::Crypto)?;
        output
            .write_all(&ciphertext)
            .map_err(|_| ProtectedFileError::Io)?;
        index = index
            .checked_add(1)
            .ok_or(ProtectedFileError::InvalidLimit)?;
    }
    output.sync_all().map_err(|_| ProtectedFileError::Io)?;
    Ok(ProtectedMediaInfo {
        plaintext_bytes,
        plaintext_sha256: hasher.finalize_hex(),
    })
}

fn read_plaintext_chunk(
    path: &Path,
    key: &[u8],
    header: &ProtectedFileHeader,
    index: u64,
) -> Result<Vec<u8>, ProtectedFileError> {
    let chunks = header.plaintext_bytes.div_ceil(header.chunk_bytes);
    if index >= chunks {
        return Err(ProtectedFileError::InvalidChunk);
    }
    let byte_offset = index
        .checked_mul(header.chunk_bytes)
        .ok_or(ProtectedFileError::InvalidLimit)?;
    let raw_bytes = header
        .plaintext_bytes
        .checked_sub(byte_offset)
        .ok_or(ProtectedFileError::InvalidLimit)?
        .min(header.chunk_bytes);
    let cipher_bytes = raw_bytes
        .checked_add(TAG_BYTES)
        .ok_or(ProtectedFileError::InvalidLimit)?;
    let cipher_offset = HEADER_LEN
        .checked_add(
            index
                .checked_mul(
                    header
                        .chunk_bytes
                        .checked_add(TAG_BYTES)
                        .ok_or(ProtectedFileError::InvalidLimit)?,
                )
                .ok_or(ProtectedFileError::InvalidLimit)?,
        )
        .ok_or(ProtectedFileError::InvalidLimit)?;
    let mut file = File::open(path).map_err(|_| ProtectedFileError::Io)?;
    file.seek(SeekFrom::Start(cipher_offset))
        .map_err(|_| ProtectedFileError::Io)?;
    let mut ciphertext =
        vec![0_u8; usize::try_from(cipher_bytes).map_err(|_| ProtectedFileError::InvalidLimit)?];
    file.read_exact(&mut ciphertext)
        .map_err(|_| ProtectedFileError::Io)?;
    cipher(key)?
        .decrypt(
            XNonce::from_slice(&nonce(header, index)),
            Payload {
                msg: &ciphertext,
                aad: &chunk_aad(header, index),
            },
        )
        .map_err(|_| ProtectedFileError::Crypto)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ProtectedFileHeader {
    plaintext_bytes: u64,
    chunk_bytes: u64,
    nonce_prefix: [u8; NONCE_PREFIX_BYTES],
}

fn write_header(output: &mut File, header: &ProtectedFileHeader) -> Result<(), ProtectedFileError> {
    output
        .write_all(MAGIC)
        .map_err(|_| ProtectedFileError::Io)?;
    output
        .write_all(&header.plaintext_bytes.to_le_bytes())
        .map_err(|_| ProtectedFileError::Io)?;
    output
        .write_all(&header.chunk_bytes.to_le_bytes())
        .map_err(|_| ProtectedFileError::Io)?;
    output
        .write_all(&header.nonce_prefix)
        .map_err(|_| ProtectedFileError::Io)
}

pub(crate) fn read_protected_file_plaintext_chunk(
    path: &Path,
    key: &[u8],
    index: u64,
) -> Result<Vec<u8>, ProtectedFileError> {
    let header = read_header(path)?;
    read_plaintext_chunk(path, key, &header, index)
}

fn read_header(path: &Path) -> Result<ProtectedFileHeader, ProtectedFileError> {
    let mut file = File::open(path).map_err(|_| ProtectedFileError::Io)?;
    let mut magic = [0_u8; 8];
    let mut plaintext_bytes = [0_u8; 8];
    let mut chunk_bytes = [0_u8; 8];
    let mut nonce_prefix = [0_u8; NONCE_PREFIX_BYTES];
    file.read_exact(&mut magic)
        .map_err(|_| ProtectedFileError::InvalidHeader)?;
    file.read_exact(&mut plaintext_bytes)
        .map_err(|_| ProtectedFileError::InvalidHeader)?;
    file.read_exact(&mut chunk_bytes)
        .map_err(|_| ProtectedFileError::InvalidHeader)?;
    file.read_exact(&mut nonce_prefix)
        .map_err(|_| ProtectedFileError::InvalidHeader)?;
    if &magic != MAGIC {
        return Err(ProtectedFileError::InvalidHeader);
    }
    let header = ProtectedFileHeader {
        plaintext_bytes: u64::from_le_bytes(plaintext_bytes),
        chunk_bytes: u64::from_le_bytes(chunk_bytes),
        nonce_prefix,
    };
    if header.plaintext_bytes == 0 || header.chunk_bytes != CHUNK_RAW_BYTES {
        return Err(ProtectedFileError::InvalidHeader);
    }
    Ok(header)
}

fn cipher(key: &[u8]) -> Result<XChaCha20Poly1305, ProtectedFileError> {
    if key.len() != KEY_BYTES {
        return Err(ProtectedFileError::InvalidKey);
    }
    XChaCha20Poly1305::new_from_slice(key).map_err(|_| ProtectedFileError::InvalidKey)
}

fn nonce(header: &ProtectedFileHeader, index: u64) -> [u8; 24] {
    let mut nonce = [0_u8; 24];
    nonce[..NONCE_PREFIX_BYTES].copy_from_slice(&header.nonce_prefix);
    nonce[NONCE_PREFIX_BYTES..].copy_from_slice(&index.to_le_bytes());
    nonce
}

fn chunk_aad(header: &ProtectedFileHeader, index: u64) -> [u8; 32] {
    let mut aad = [0_u8; 32];
    aad[..8].copy_from_slice(MAGIC);
    aad[8..16].copy_from_slice(&header.plaintext_bytes.to_le_bytes());
    aad[16..24].copy_from_slice(&header.chunk_bytes.to_le_bytes());
    aad[24..].copy_from_slice(&index.to_le_bytes());
    aad
}

fn protected_temp_path(path: &Path) -> Result<PathBuf, ProtectedFileError> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(ProtectedFileError::InvalidHeader)?;
    Ok(path.with_file_name(format!("{file_name}.protected.tmp")))
}
