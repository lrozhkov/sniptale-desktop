use crate::{
    protocol::{
        DeliveryRoute,
        trajectory::{CursorTrajectoryBlock, CursorTrajectoryState, encode_trajectory_block},
    },
    transfer::{
        hashing::sha256_hex,
        trajectory_store::{TrajectoryArchive, TrajectoryStoreError, TrajectoryWriter},
    },
};
use std::{
    fs,
    path::{Path, PathBuf},
};

type ProofError = Box<dyn std::error::Error>;
const RECORDING: &str = "recording-fixture";
const TRAJECTORY: &str = "0123456789abcdef0123456789abcdef";

mod paging;
mod receipts;
mod recovery;

fn fixture() -> Result<(CursorTrajectoryBlock, Vec<u8>), ProofError> {
    let value: serde_json::Value = serde_json::from_str(include_str!(
        "../../../contracts/native/fixtures/cursor-trajectory-v1.json"
    ))?;
    let block = serde_json::from_value(value["canonicalVectors"][0]["value"].clone())?;
    let bytes = encode_trajectory_block(&block).map_err(|error| format!("{error:?}"))?;
    Ok((block, bytes))
}

fn create(root: &Path) -> Result<TrajectoryWriter, TrajectoryStoreError> {
    TrajectoryWriter::create(
        root,
        RECORDING,
        TRAJECTORY,
        DeliveryRoute::DesktopDelivery {
            asset_job_id: "job-fixture".to_owned(),
            target_id: Some("a".repeat(64)),
            ask_after_capture: false,
        },
    )
}

fn directory(root: &Path) -> PathBuf {
    root.join(format!("{}-{TRAJECTORY}", sha256_hex(RECORDING.as_bytes())))
}

#[test]
fn encrypted_history_round_trips_and_exact_retry_cannot_change_final_descriptor()
-> Result<(), ProofError> {
    let root = tempfile::tempdir()?;
    let (block, bytes) = fixture()?;
    let mut writer = create(root.path())?;
    assert!(matches!(
        TrajectoryArchive::open(root.path(), RECORDING, TRAJECTORY),
        Err(TrajectoryStoreError::Busy)
    ));
    writer.append(&bytes)?;
    writer.append(&bytes)?;
    let descriptor = writer.finalize(false)?;
    assert_eq!(descriptor.state, CursorTrajectoryState::Complete);
    assert_eq!(descriptor.total_blocks, 1);
    assert_eq!(descriptor.total_samples, 2);
    assert_eq!(descriptor.total_records, 4);
    assert_eq!(descriptor.total_bytes, 837);
    assert_eq!(writer.finalize(true)?, descriptor);
    let mut conflicting = block;
    conflicting.end_time_us = 100_001;
    if let crate::protocol::trajectory::CursorTrajectoryRecord::Boundary(boundary) =
        &mut conflicting.records[3]
    {
        boundary.active_time_us = 100_001;
    }
    let conflicting =
        encode_trajectory_block(&conflicting).map_err(|error| format!("{error:?}"))?;
    assert_eq!(
        writer.append(&conflicting),
        Err(TrajectoryStoreError::Conflict)
    );
    drop(writer);
    let disk = fs::read(directory(root.path()).join("blocks.bin"))?;
    assert_eq!(disk.len(), bytes.len() + 52);
    assert!(!disk.windows(8).any(|part| part == b"segmentI"));
    let mut archive = TrajectoryArchive::open(root.path(), RECORDING, TRAJECTORY)?;
    assert_eq!(archive.descriptor(), &descriptor);
    assert_eq!(archive.read_block(0)?, bytes);
    let page = archive.manifest(0, &sha256_hex(&[]))?;
    assert_eq!(page.len(), 1);
    assert_eq!(page[0].block_sha256, sha256_hex(&bytes));
    assert_eq!(page[0].prefix_sha256, descriptor.stream_sha256);
    assert_eq!(archive.manifest(1, &descriptor.stream_sha256)?, []);
    assert!(archive.manifest(2, &descriptor.stream_sha256).is_err());
    assert!(archive.read_block(1).is_err());
    Ok(())
}

#[test]
fn failed_head_commit_never_exposes_orphan_tail_or_allows_later_append() -> Result<(), ProofError> {
    let root = tempfile::tempdir()?;
    let (_, bytes) = fixture()?;
    let mut writer = create(root.path())?;
    fs::create_dir(directory(root.path()).join("head.pending"))?;
    assert_eq!(writer.append(&bytes), Err(TrajectoryStoreError::Io));
    assert_eq!(writer.append(&bytes), Err(TrajectoryStoreError::Retired));
    assert!(writer.finalize(false).is_err());
    drop(writer);
    let mut archive = TrajectoryArchive::open(root.path(), RECORDING, TRAJECTORY)?;
    assert_eq!(archive.descriptor().total_blocks, 0);
    assert!(!archive.descriptor().finalized);
    assert_eq!(archive.manifest(0, &sha256_hex(&[]))?, []);
    assert!(archive.read_block(0).is_err());
    Ok(())
}

#[test]
fn persisted_index_head_and_ciphertext_tampering_fail_closed() -> Result<(), ProofError> {
    for name in ["head.json", "index.bin", "blocks.bin"] {
        let root = tempfile::tempdir()?;
        let (_, bytes) = fixture()?;
        let mut writer = create(root.path())?;
        writer.append(&bytes)?;
        writer.finalize(false)?;
        drop(writer);
        let path = directory(root.path()).join(name);
        let mut stored = fs::read(&path)?;
        let last = stored.len() - 1;
        stored[last] ^= 1;
        fs::write(&path, stored)?;
        let opened = TrajectoryArchive::open(root.path(), RECORDING, TRAJECTORY);
        if name == "blocks.bin" {
            assert!(opened?.read_block(0).is_err());
        } else {
            assert!(opened.is_err());
        }
    }
    Ok(())
}
