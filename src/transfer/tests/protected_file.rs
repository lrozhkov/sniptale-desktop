use std::io::Read;

use super::super::{
    chunks::MediaTransferKind,
    error::TransferError,
    hashing::sha256_hex,
    protected_file::{
        PROTECTED_MEDIA_ALGORITHM, encode_protected_file_chunk, protect_file_in_place,
    },
    protected_key::{new_protected_media_key, unprotect_media_key},
    protected_read::{ProtectedMediaReader, read_protected_media_chunk},
};

#[test]
fn protected_file_preserves_plaintext_chunk_contract() -> Result<(), TransferError> {
    let temp = tempfile::tempdir().map_err(|_| TransferError::FileIo)?;
    let path = temp.path().join("recording-1.mp4");
    std::fs::write(&path, b"hello world").map_err(|_| TransferError::FileIo)?;
    let key = [7_u8; 32];
    let nonce_prefix = [9_u8; 16];

    let info = protect_file_in_place(&path, &key, nonce_prefix)
        .map_err(|_| TransferError::ProtectedMedia)?;

    assert_eq!(info.plaintext_bytes, 11);
    assert_eq!(info.plaintext_sha256, sha256_hex(b"hello world"));
    assert_ne!(read_test_file(&path)?, b"hello world");
    let chunk = encode_protected_file_chunk(MediaTransferKind::Recording, &path, &key, 11, 0, 128)?;
    assert_eq!(chunk.raw_bytes, 11);
    assert_eq!(chunk.chunk_sha256, sha256_hex(b"hello world"));
    assert_eq!(chunk.base64, "aGVsbG8gd29ybGQ=");
    Ok(())
}

#[test]
fn protected_media_read_api_enforces_bounds_and_reads_plaintext_contract()
-> Result<(), TransferError> {
    let temp = tempfile::tempdir().map_err(|_| TransferError::FileIo)?;
    let path = temp.path().join("capture-1.png");
    std::fs::write(&path, b"hello world").map_err(|_| TransferError::FileIo)?;
    let (protection, key) = new_protected_media_key(PROTECTED_MEDIA_ALGORITHM)
        .map_err(|_| TransferError::ProtectedMedia)?;
    let (_, nonce_prefix) = unprotect_media_key(&protection, PROTECTED_MEDIA_ALGORITHM)
        .map_err(|_| TransferError::ProtectedMedia)?;
    protect_file_in_place(&path, &key, nonce_prefix).map_err(|_| TransferError::ProtectedMedia)?;

    let chunk = read_protected_media_chunk(
        MediaTransferKind::Screenshot,
        &path,
        &protection,
        11,
        0,
        128,
    )?;

    assert_eq!(chunk.raw_bytes, 11);
    assert_eq!(chunk.chunk_sha256, sha256_hex(b"hello world"));
    assert_eq!(chunk.base64, "aGVsbG8gd29ybGQ=");
    assert_eq!(
        read_protected_media_chunk(
            MediaTransferKind::Screenshot,
            &path,
            &protection,
            11,
            1,
            128
        ),
        Err(TransferError::ChunkIndexOutOfBounds {
            index: 1,
            chunk_count: 1
        })
    );
    Ok(())
}

#[test]
fn protected_file_rejects_tampered_ciphertext() -> Result<(), TransferError> {
    use std::io::{Seek, SeekFrom, Write};

    let temp = tempfile::tempdir().map_err(|_| TransferError::FileIo)?;
    let path = temp.path().join("recording-1.mp4");
    std::fs::write(&path, b"hello world").map_err(|_| TransferError::FileIo)?;
    let key = [7_u8; 32];
    protect_file_in_place(&path, &key, [9_u8; 16]).map_err(|_| TransferError::ProtectedMedia)?;
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .map_err(|_| TransferError::FileIo)?;
    file.seek(SeekFrom::End(-1))
        .map_err(|_| TransferError::FileIo)?;
    let mut byte = [0_u8; 1];
    file.read_exact(&mut byte)
        .map_err(|_| TransferError::FileIo)?;
    file.seek(SeekFrom::End(-1))
        .map_err(|_| TransferError::FileIo)?;
    file.write_all(&[byte[0] ^ 0xff])
        .map_err(|_| TransferError::FileIo)?;

    assert_eq!(
        encode_protected_file_chunk(MediaTransferKind::Recording, &path, &key, 11, 0, 128),
        Err(TransferError::ProtectedMedia)
    );
    Ok(())
}

#[test]
fn protected_media_reader_keeps_key_and_path_inside_transfer_owner() -> Result<(), TransferError> {
    let temp = tempfile::tempdir().map_err(|_| TransferError::FileIo)?;
    let path = temp.path().join("recording-1.mp4");
    std::fs::write(&path, b"hello world").map_err(|_| TransferError::FileIo)?;
    let (protection, key) = new_protected_media_key(PROTECTED_MEDIA_ALGORITHM)
        .map_err(|_| TransferError::ProtectedMedia)?;
    let (_, nonce_prefix) = unprotect_media_key(&protection, PROTECTED_MEDIA_ALGORITHM)
        .map_err(|_| TransferError::ProtectedMedia)?;
    protect_file_in_place(&path, &key, nonce_prefix).map_err(|_| TransferError::ProtectedMedia)?;

    let mut reader = ProtectedMediaReader::open(path, &protection, 11, sha256_hex(b"hello world"))?;
    assert_eq!(reader.total_bytes(), 11);
    assert_eq!(reader.sha256(), sha256_hex(b"hello world"));
    assert_eq!(
        reader.read_next_plaintext(64)?,
        Some(b"hello world".to_vec())
    );
    assert_eq!(reader.read_next_plaintext(64)?, None);
    Ok(())
}

#[test]
fn protected_media_reader_subdivides_encryption_chunks_for_bounded_consumers()
-> Result<(), TransferError> {
    let temp = tempfile::tempdir().map_err(|_| TransferError::FileIo)?;
    let path = temp.path().join("large-screenshot.png");
    let plaintext = vec![0x5a_u8; 600 * 1024];
    std::fs::write(&path, &plaintext).map_err(|_| TransferError::FileIo)?;
    let (protection, key) = new_protected_media_key(PROTECTED_MEDIA_ALGORITHM)
        .map_err(|_| TransferError::ProtectedMedia)?;
    let (_, nonce_prefix) = unprotect_media_key(&protection, PROTECTED_MEDIA_ALGORITHM)
        .map_err(|_| TransferError::ProtectedMedia)?;
    protect_file_in_place(&path, &key, nonce_prefix).map_err(|_| TransferError::ProtectedMedia)?;
    let total_bytes = u64::try_from(plaintext.len()).map_err(|_| TransferError::InvalidLimit)?;
    let mut reader =
        ProtectedMediaReader::open(path, &protection, total_bytes, sha256_hex(&plaintext))?;
    let mut restored = Vec::new();
    while let Some(chunk) = reader.read_next_plaintext(64 * 1024)? {
        assert!(chunk.len() <= 64 * 1024);
        restored.extend_from_slice(&chunk);
    }
    assert_eq!(restored, plaintext);
    Ok(())
}

fn read_test_file(path: &std::path::Path) -> Result<Vec<u8>, TransferError> {
    let mut file = std::fs::File::open(path).map_err(|_| TransferError::FileIo)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|_| TransferError::FileIo)?;
    Ok(bytes)
}
