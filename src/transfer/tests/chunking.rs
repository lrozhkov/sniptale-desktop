use super::super::{
    chunks::{
        EncodedChunk, MediaTransferKind, chunk_count, encode_chunk, raw_chunk_limit,
        validate_base64_len,
    },
    error::TransferError,
    hashing::sha256_hex,
};
use crate::protocol::{
    CHUNK_MAX_BASE64_CHARS, CHUNK_RAW_BYTES, MESSAGE_TARGET_MAX_JSON_UTF8_BYTES,
    SCREENSHOT_MAX_TOTAL_BYTES,
};
use proptest::{prelude::*, test_runner::TestCaseError};

fn bounded_chunk_case() -> impl Strategy<Value = (u64, u64, Vec<u8>)> {
    (1_u64..=(CHUNK_RAW_BYTES * 2 + 17)).prop_flat_map(|total_bytes| {
        let chunk_count = total_bytes.div_ceil(CHUNK_RAW_BYTES);
        (Just(total_bytes), 0_u64..chunk_count).prop_map(|(total_bytes, index)| {
            let byte_offset = index * CHUNK_RAW_BYTES;
            let expected_raw_bytes = total_bytes.saturating_sub(byte_offset).min(CHUNK_RAW_BYTES);
            let raw = usize::try_from(expected_raw_bytes)
                .ok()
                .map_or_else(Vec::new, |len| vec![0x5a; len]);
            (total_bytes, index, raw)
        })
    })
}

proptest! {
    #[test]
    fn chunk_count_matches_ceiling_division_for_non_empty_transfers(
        total_bytes in 1_u64..=1_000_000
    ) {
        prop_assert_eq!(chunk_count(total_bytes), Ok(total_bytes.div_ceil(CHUNK_RAW_BYTES)));
    }

    #[test]
    fn encode_chunk_tracks_offsets_hashes_and_budgets(
        (total_bytes, index, raw) in bounded_chunk_case()
    ) {
        let chunk = encode_chunk(MediaTransferKind::Recording, total_bytes, index, &raw, 128)
            .map_err(|error| TestCaseError::fail(format!("{error:?}")))?;
        let raw_len = u64::try_from(raw.len())
            .map_err(|_| TestCaseError::fail("raw length overflow"))?;

        prop_assert_eq!(chunk.index, index);
        prop_assert_eq!(chunk.byte_offset, index * CHUNK_RAW_BYTES);
        prop_assert_eq!(chunk.raw_bytes, raw_len);
        prop_assert_eq!(chunk.chunk_sha256, sha256_hex(&raw));
        prop_assert!(chunk.base64.len() <= CHUNK_MAX_BASE64_CHARS);
    }
}

#[test]
fn encodes_chunk_with_offsets_hash_and_base64() -> Result<(), TransferError> {
    let chunk = encode_chunk(MediaTransferKind::Screenshot, 11, 0, b"hello world", 128)?;

    assert_eq!(
        chunk,
        EncodedChunk {
            index: 0,
            byte_offset: 0,
            raw_bytes: 11,
            chunk_sha256: sha256_hex(b"hello world"),
            base64: "aGVsbG8gd29ybGQ=".to_owned(),
        }
    );
    Ok(())
}

#[test]
fn computes_exact_chunk_count_and_last_chunk_size() -> Result<(), TransferError> {
    let limit = raw_chunk_limit()?;
    let total = u64::try_from(limit).map_err(|_| TransferError::InvalidLimit)? + 1;
    let first = vec![1_u8; limit];
    let last = [2_u8];

    assert_eq!(chunk_count(total)?, 2);
    assert_eq!(
        encode_chunk(MediaTransferKind::Recording, total, 0, &first, 128)?.raw_bytes,
        u64::try_from(limit).map_err(|_| TransferError::InvalidLimit)?
    );
    assert_eq!(
        encode_chunk(MediaTransferKind::Recording, total, 1, &last, 128)?.byte_offset,
        u64::try_from(limit).map_err(|_| TransferError::InvalidLimit)?
    );
    Ok(())
}

#[test]
fn rejects_empty_and_total_oversize_transfers() {
    assert!(matches!(chunk_count(0), Err(TransferError::EmptyTransfer)));
    assert!(matches!(
        encode_chunk(
            MediaTransferKind::Screenshot,
            SCREENSHOT_MAX_TOTAL_BYTES + 1,
            0,
            b"x",
            128
        ),
        Err(TransferError::TotalBytesTooLarge { .. })
    ));
}

#[test]
fn rejects_wrong_chunk_index_and_size() {
    assert!(matches!(
        encode_chunk(MediaTransferKind::Screenshot, 3, 1, b"abc", 128),
        Err(TransferError::ChunkIndexOutOfBounds { .. })
    ));
    assert!(matches!(
        encode_chunk(MediaTransferKind::Screenshot, 3, 0, b"ab", 128),
        Err(TransferError::ChunkSizeMismatch {
            expected: 3,
            actual: 2
        })
    ));
}

#[test]
fn enforces_raw_base64_and_json_message_budgets() -> Result<(), TransferError> {
    let limit = raw_chunk_limit()?;
    let too_large = vec![0_u8; limit + 1];
    assert!(matches!(
        encode_chunk(
            MediaTransferKind::Recording,
            u64::try_from(limit + 1).map_err(|_| TransferError::InvalidLimit)?,
            0,
            &too_large,
            128
        ),
        Err(TransferError::ChunkRawBytesTooLarge { .. })
    ));

    assert!(matches!(
        validate_base64_len(CHUNK_MAX_BASE64_CHARS + 1),
        Err(TransferError::Base64TooLarge { .. })
    ));

    let raw = vec![3_u8; limit];
    assert!(matches!(
        encode_chunk(
            MediaTransferKind::Recording,
            u64::try_from(limit).map_err(|_| TransferError::InvalidLimit)?,
            0,
            &raw,
            MESSAGE_TARGET_MAX_JSON_UTF8_BYTES
        ),
        Err(TransferError::JsonMessageTooLarge { .. })
    ));
    Ok(())
}
