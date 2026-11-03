use super::{ProofError, RECORDING, TRAJECTORY, create, directory, fixture};
use crate::{
    protocol::trajectory::CursorTrajectoryState, transfer::trajectory_store::TrajectoryArchive,
};

#[test]
fn interrupted_committed_prefix_is_sealed_incomplete_only_by_explicit_recovery()
-> Result<(), ProofError> {
    let root = tempfile::tempdir()?;
    let (_, bytes) = fixture()?;
    let mut writer = create(root.path())?;
    writer.append(&bytes)?;
    drop(writer);
    let before = std::fs::read(directory(root.path()).join("head.json"))?;
    let mut archive = TrajectoryArchive::open(root.path(), RECORDING, TRAJECTORY)?;
    assert!(!archive.descriptor().finalized);
    assert_eq!(archive.read_block(0)?, bytes);
    assert_eq!(
        std::fs::read(directory(root.path()).join("head.json"))?,
        before
    );
    archive.recover_interrupted()?;
    let recovered = archive.descriptor().clone();
    assert!(recovered.finalized);
    assert_eq!(recovered.state, CursorTrajectoryState::Incomplete);
    assert_eq!(recovered.total_blocks, 1);
    assert_eq!(recovered.total_bytes, 837);
    archive.recover_interrupted()?;
    assert_eq!(archive.descriptor(), &recovered);
    drop(archive);
    assert_eq!(
        TrajectoryArchive::open(root.path(), RECORDING, TRAJECTORY)?.descriptor(),
        &recovered
    );
    Ok(())
}

#[test]
fn small_block_chunk_is_exact_and_out_of_range_request_is_refused() -> Result<(), ProofError> {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    let root = tempfile::tempdir()?;
    let (_, bytes) = fixture()?;
    let mut writer = create(root.path())?;
    writer.append(&bytes)?;
    writer.finalize(false)?;
    let mut archive = writer.into_archive();
    let chunk = archive.chunk(0, 0)?;
    assert_eq!(chunk.chunk_count, 1);
    assert_eq!(chunk.chunk_raw_bytes, 837);
    assert_eq!(chunk.chunk_byte_offset, 0);
    assert_eq!(chunk.block_bytes, 837);
    assert_eq!(STANDARD.decode(chunk.base64)?, bytes);
    assert_eq!(chunk.block_sha256, chunk.chunk_sha256);
    let entry = archive.block_entry(0)?;
    assert_eq!(entry.sequence, 0);
    assert_eq!(entry.byte_length, chunk.block_bytes);
    assert_eq!(entry.block_sha256, chunk.block_sha256);
    assert!(archive.block_entry(1).is_err());
    assert!(archive.chunk(0, 1).is_err());
    assert!(archive.chunk(1, 0).is_err());
    Ok(())
}

#[test]
fn concurrent_readers_cannot_mutate_the_journal_or_overlap_a_writer() -> Result<(), ProofError> {
    let root = tempfile::tempdir()?;
    let (_, bytes) = fixture()?;
    let mut writer = create(root.path())?;
    writer.append(&bytes)?;
    writer.finalize(false)?;
    drop(writer);
    let mut first = TrajectoryArchive::read(root.path(), RECORDING, TRAJECTORY)?;
    let mut second = TrajectoryArchive::read(root.path(), RECORDING, TRAJECTORY)?;
    assert_eq!(first.read_block(0)?, bytes);
    assert_eq!(second.read_block(0)?, bytes);
    assert!(TrajectoryArchive::open(root.path(), RECORDING, TRAJECTORY).is_err());
    assert!(first.recover_interrupted().is_err());
    drop(first);
    drop(second);
    assert!(TrajectoryArchive::open(root.path(), RECORDING, TRAJECTORY).is_ok());
    Ok(())
}

#[cfg(unix)]
#[test]
fn pending_head_symlink_cannot_overwrite_another_file() -> Result<(), ProofError> {
    use std::os::unix::fs::symlink;
    let root = tempfile::tempdir()?;
    let (_, bytes) = fixture()?;
    let mut writer = create(root.path())?;
    let victim = root.path().join("unrelated");
    std::fs::write(&victim, b"keep")?;
    symlink(&victim, directory(root.path()).join("head.pending"))?;
    assert!(writer.append(&bytes).is_err());
    assert_eq!(std::fs::read(victim)?, b"keep");
    Ok(())
}

#[test]
fn catalog_recovery_skips_live_writers_and_rejects_a_relocated_authenticated_prefix()
-> Result<(), ProofError> {
    use crate::transfer::{
        browser_delivery::BrowserDeliveryStore, trajectory_catalog::TrajectoryCatalog,
        trajectory_store::TrajectoryStoreError,
    };
    let root = tempfile::tempdir()?;
    let history = root.path().join("history");
    let catalog = TrajectoryCatalog::open(history.clone());
    let assignments = BrowserDeliveryStore::open(root.path().join("assignments"));
    catalog.recover_interrupted()?;
    assert!(catalog.lookup(RECORDING, TRAJECTORY)?.is_none());
    assert_eq!(
        catalog
            .pending_after_video_ack(&"a".repeat(64), &assignments)?
            .len(),
        0
    );
    let mut writer = create(&history)?;
    let (_, bytes) = fixture()?;
    writer.append(&bytes)?;
    std::fs::write(history.join("ignored-file"), [])?;
    std::fs::create_dir(history.join("incomplete-directory"))?;
    catalog.recover_interrupted()?;
    assert!(!writer.descriptor().finalized);
    drop(writer);
    let path = directory(&history);
    std::fs::rename(&path, history.join("relocated-source"))?;
    assert!(matches!(
        catalog.recover_interrupted(),
        Err(TrajectoryStoreError::Invalid)
    ));
    std::fs::rename(history.join("relocated-source"), &path)?;
    catalog.recover_interrupted()?;
    let state = catalog
        .lookup(RECORDING, TRAJECTORY)?
        .ok_or("source")?
        .descriptor;
    assert!(state.finalized);
    assert_eq!(state.state, CursorTrajectoryState::Incomplete);
    catalog.recover_interrupted()?;
    assert_eq!(
        catalog
            .lookup(RECORDING, TRAJECTORY)?
            .ok_or("source")?
            .descriptor,
        state
    );
    Ok(())
}
