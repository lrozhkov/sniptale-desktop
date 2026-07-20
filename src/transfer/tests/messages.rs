use super::super::{
    chunks::EncodedChunk,
    hashing::sha256_hex,
    messages::{recording_chunk_value, screenshot_chunk_value},
};
use crate::protocol::CHUNK_RAW_BYTES;

#[test]
fn transfer_chunk_message_builders_preserve_chunk_metadata()
-> Result<(), Box<dyn std::error::Error>> {
    let chunk = EncodedChunk {
        index: 1,
        byte_offset: CHUNK_RAW_BYTES,
        raw_bytes: 11,
        chunk_sha256: sha256_hex(b"hello world"),
        base64: "aGVsbG8gd29ybGQ=".to_owned(),
    };

    let screenshot = screenshot_chunk_value("lease-1", "capture-1", chunk.clone())?;
    assert_eq!(screenshot["type"], "app.screenshot.chunk");
    assert_eq!(screenshot["controllerLeaseId"], "lease-1");
    assert_eq!(screenshot["captureId"], "capture-1");
    assert_eq!(screenshot["chunkIndex"], 1);
    assert_eq!(screenshot["chunkByteOffset"], CHUNK_RAW_BYTES);
    assert_eq!(screenshot["chunkRawBytes"], 11);
    assert_eq!(screenshot["chunkSha256"], sha256_hex(b"hello world"));
    assert_eq!(screenshot["base64"], "aGVsbG8gd29ybGQ=");

    let recording = recording_chunk_value("lease-1", "recording-1", chunk)?;
    assert_eq!(recording["type"], "app.recording.chunk");
    assert_eq!(recording["controllerLeaseId"], "lease-1");
    assert_eq!(recording["recordingId"], "recording-1");
    assert_eq!(recording["chunkIndex"], 1);
    assert_eq!(recording["chunkByteOffset"], CHUNK_RAW_BYTES);
    assert_eq!(recording["chunkRawBytes"], 11);
    assert_eq!(recording["chunkSha256"], sha256_hex(b"hello world"));
    assert_eq!(recording["base64"], "aGVsbG8gd29ybGQ=");
    Ok(())
}
