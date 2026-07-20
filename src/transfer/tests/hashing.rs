use super::super::chunks::MediaTransferKind;
use super::super::{
    error::TransferError,
    file::{encode_file_chunk, sha256_file_hex},
    hashing::{Sha256Accumulator, sha256_hex},
};

#[test]
fn hashes_match_contract_fixture_payload() {
    assert_eq!(
        sha256_hex(b"hello world"),
        "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
    );
}

#[test]
fn streaming_hash_matches_single_pass_hash() {
    let mut accumulator = Sha256Accumulator::new();
    accumulator.update(b"hello ");
    accumulator.update(b"world");

    assert_eq!(accumulator.finalize_hex(), sha256_hex(b"hello world"));
}

#[test]
fn encodes_file_chunk_without_reading_unbounded_media() -> Result<(), TransferError> {
    let temp = tempfile::tempdir().map_err(|_| TransferError::FileIo)?;
    let path = temp.path().join("recording-1.mp4");
    std::fs::write(&path, b"hello world").map_err(|_| TransferError::FileIo)?;

    let chunk = encode_file_chunk(MediaTransferKind::Recording, &path, 11, 0, 128)?;

    assert_eq!(chunk.raw_bytes, 11);
    assert_eq!(chunk.chunk_sha256, sha256_hex(b"hello world"));
    assert_eq!(chunk.base64, "aGVsbG8gd29ybGQ=");
    Ok(())
}

#[test]
fn hashes_file_with_streaming_reader() -> Result<(), TransferError> {
    let temp = tempfile::tempdir().map_err(|_| TransferError::FileIo)?;
    let path = temp.path().join("capture-1.png");
    std::fs::write(&path, b"hello world").map_err(|_| TransferError::FileIo)?;

    assert_eq!(sha256_file_hex(&path)?, sha256_hex(b"hello world"));
    Ok(())
}
