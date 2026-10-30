use super::*;

#[test]
fn finalized_temp_media_reports_pending_until_ack_or_ttl() -> Result<(), TempStoreError> {
    let mut record = TempMediaRecord::start_writing("recording-1", 1_000)?;
    record.finalize(
        1024,
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        2_000,
    )?;

    assert_eq!(
        record.recovery_decision(3_000, 500, 60_000)?,
        StartupRecoveryDecision::ReportPending
    );
    assert_eq!(
        record.recovery_decision(70_001, 500, 60_000)?,
        StartupRecoveryDecision::Delete
    );

    record.ack(70_002)?;
    assert_eq!(
        record.recovery_decision(70_003, 500, 60_000)?,
        StartupRecoveryDecision::Delete
    );
    Ok(())
}

#[test]
fn temp_media_reject_can_retain_diagnostics_but_never_replay() -> Result<(), TempStoreError> {
    let mut record = TempMediaRecord::start_writing("recording-1", 1_000)?;
    record.finalize(
        1024,
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        2_000,
    )?;
    record.reject(3_000, Some(10_000))?;

    assert_eq!(
        record.recovery_decision(4_000, 500, 60_000)?,
        StartupRecoveryDecision::RetainRejected
    );
    assert_eq!(
        record.recovery_decision(10_000, 500, 60_000)?,
        StartupRecoveryDecision::Delete
    );
    Ok(())
}

#[test]
fn temp_media_finalization_requires_size_and_hash_before_transfer() -> Result<(), TempStoreError> {
    let mut record = TempMediaRecord::start_writing("recording-1", 1_000)?;

    assert_eq!(
        record.finalize(
            0,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            2_000
        ),
        Err(TempStoreError::InvalidSize)
    );
    assert_eq!(
        record.finalize(1, "not-a-sha", 2_000),
        Err(TempStoreError::InvalidSha256)
    );
    assert_eq!(
        record.recovery_decision(1_500, 500, 60_000)?,
        StartupRecoveryDecision::KeepWriting
    );
    assert_eq!(
        record.recovery_decision(1_501, 500, 60_000)?,
        StartupRecoveryDecision::Delete
    );
    Ok(())
}

#[test]
fn disk_temp_store_reports_finalized_pending_and_deletes_terminal_records()
-> Result<(), TempStoreDiskError> {
    let temp = tempfile::tempdir().map_err(|_| TempStoreDiskError::Io)?;
    let store = TempMediaDiskStore::open(temp.path())?;
    let mut pending = TempMediaRecord::start_writing("recording-1", 1_000)
        .map_err(TempStoreDiskError::InvalidMedia)?;
    pending
        .finalize(
            1024,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            2_000,
        )
        .map_err(TempStoreDiskError::InvalidMedia)?;
    let mut acked = TempMediaRecord::start_writing("recording-2", 1_000)
        .map_err(TempStoreDiskError::InvalidMedia)?;
    acked
        .finalize(
            1024,
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            2_000,
        )
        .map_err(TempStoreDiskError::InvalidMedia)?;
    acked.ack(2_500).map_err(TempStoreDiskError::InvalidMedia)?;

    store.write_record(&pending)?;
    store.write_record(&acked)?;
    let kept = store.cleanup_expired(3_000, 500, 60_000)?;

    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].record.media_id, "recording-1");
    assert_eq!(kept[0].decision, StartupRecoveryDecision::ReportPending);
    assert!(temp.path().join("recording-1.json").is_file());
    assert!(!temp.path().join("recording-2.json").exists());
    Ok(())
}

#[test]
fn disk_temp_store_cleanup_removes_expired_record_and_media() -> Result<(), TempStoreDiskError> {
    let temp = tempfile::tempdir().map_err(|_| TempStoreDiskError::Io)?;
    let store = TempMediaDiskStore::open(temp.path())?;
    let expired = TempMediaRecord::start_writing("recording-1", 1_000)
        .map_err(TempStoreDiskError::InvalidMedia)?;
    store.write_record(&expired)?;
    std::fs::write(temp.path().join("recording-1.mp4"), b"incomplete")
        .map_err(|_| TempStoreDiskError::Io)?;

    let kept = store.cleanup_expired(3_000, 500, 60_000)?;

    assert_eq!(kept.len(), 0);
    assert!(!temp.path().join("recording-1.json").exists());
    assert!(!temp.path().join("recording-1.mp4").exists());
    Ok(())
}

#[test]
fn reconnect_reporting_only_includes_finalized_pending_recordings() -> Result<(), TempStoreError> {
    let mut finalized = TempMediaRecord::start_writing("recording-2", 1_000)?;
    finalized.finalize_recording(
        crate::protocol::CHUNK_RAW_BYTES + 1,
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        recording_metadata("recording-2.mp4"),
        2_000,
    )?;
    let writing = TempMediaRecord::start_writing("recording-1", 1_000)?;
    let mut acked = TempMediaRecord::start_writing("recording-3", 1_000)?;
    acked.finalize(
        10,
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        2_000,
    )?;
    acked.ack(3_000)?;

    let recovered = vec![
        recovered(writing, StartupRecoveryDecision::KeepWriting),
        recovered(finalized, StartupRecoveryDecision::ReportPending),
        recovered(acked, StartupRecoveryDecision::Delete),
    ];

    let pending = pending_recordings_for_reconnect(&recovered)
        .map_err(|_| TempStoreError::InvalidTransition)?;

    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].recording_id, "recording-2");
    assert_eq!(pending[0].filename, "recording-2.mp4");
    assert_eq!(pending[0].duration_ms, 4_000);
    assert_eq!(pending[0].width, 1920);
    assert_eq!(pending[0].height, 1080);
    assert_eq!(pending[0].fps, 30);
    assert_eq!(pending[0].chunk_count, 2);
    assert_eq!(pending[0].next_missing_chunk_index, 0);
    assert!(pending[0].open_editor);
    assert_eq!(
        pending[0].sha256,
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    );
    Ok(())
}

#[test]
fn reconnect_reporting_rejects_pending_records_without_finalized_metadata()
-> Result<(), TempStoreError> {
    let writing = TempMediaRecord::start_writing("recording-1", 1_000)?;
    let recovered = vec![recovered(writing, StartupRecoveryDecision::ReportPending)];

    assert_eq!(
        pending_recordings_for_reconnect(&recovered),
        Err(RecordingRecoveryError::MissingFinalizedMetadata)
    );
    Ok(())
}

#[test]
fn finalized_recording_metadata_rejects_unsafe_filenames() -> Result<(), TempStoreError> {
    for filename in ["../recording-1.mp4", "con.mp4"] {
        let mut record = TempMediaRecord::start_writing("recording-1", 1_000)?;
        let mut metadata = recording_metadata("recording-1.mp4");
        metadata.filename = filename.to_owned();

        assert_eq!(
            record.finalize_recording(
                1024,
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                metadata,
                2_000,
            ),
            Err(TempStoreError::InvalidFilename)
        );
    }
    Ok(())
}

#[test]
fn disk_temp_store_rejects_path_injection_media_ids() {
    assert_eq!(
        TempMediaRecord::start_writing("../recording", 1_000),
        Err(TempStoreError::InvalidMediaId)
    );
    assert_eq!(
        TempMediaRecord::start_writing("recording:ads", 1_000),
        Err(TempStoreError::InvalidMediaId)
    );
}

fn recovered(record: TempMediaRecord, decision: StartupRecoveryDecision) -> RecoveredTempMedia {
    RecoveredTempMedia { record, decision }
}

fn recording_metadata(filename: &str) -> RecordingMediaMetadata {
    RecordingMediaMetadata {
        filename: filename.to_owned(),
        duration_ms: 4_000,
        width: 1920,
        height: 1080,
        fps: 30,
        open_editor: true,
    }
}

#[test]
fn recording_transfer_store_reads_pending_chunks_from_temp_media()
-> Result<(), RecordingTransferStoreError> {
    let temp = tempfile::tempdir()
        .map_err(|_| RecordingTransferStoreError::Disk(TempStoreDiskError::Io))?;
    let store = RecordingTransferStore::open(temp.path(), RecordingTransferStoreConfig::default())?;
    let media_path = store.start_recording_at("recording-1", 1_000)?;
    std::fs::write(&media_path, b"hello world")
        .map_err(|_| RecordingTransferStoreError::Disk(TempStoreDiskError::Io))?;
    store.finalize_recording_at("recording-1", recording_metadata("recording-1.mp4"), 2_000)?;

    let chunk = store.recording_chunk_at("recording-1", 0, 3_000)?.ok_or(
        RecordingTransferStoreError::Transfer(crate::transfer::error::TransferError::EmptyTransfer),
    )?;

    assert_eq!(chunk.raw_bytes, 11);
    assert_eq!(chunk.chunk_sha256, sha256_hex(b"hello world"));
    assert_eq!(chunk.base64, "aGVsbG8gd29ybGQ=");
    let pending = store.pending_recordings_for_reconnect_at(3_000)?;
    assert_eq!(pending[0].next_missing_chunk_index, 1);
    assert!(matches!(
        store.recording_chunk_at("recording-1", 0, 3_000),
        Err(RecordingTransferStoreError::Transfer(
            crate::transfer::error::TransferError::OutOfOrderChunk {
                expected: 1,
                actual: 0
            }
        ))
    ));
    assert!(matches!(
        store.recording_chunk_at("recording-1", 1, 3_000),
        Err(RecordingTransferStoreError::Transfer(
            crate::transfer::error::TransferError::ChunkIndexOutOfBounds {
                index: 1,
                chunk_count: 1
            }
        ))
    ));
    Ok(())
}

#[test]
fn recording_finalize_chunk_read_path_handles_bounded_multi_chunk_media()
-> Result<(), RecordingTransferStoreError> {
    let temp = tempfile::tempdir()
        .map_err(|_| RecordingTransferStoreError::Disk(TempStoreDiskError::Io))?;
    let store = RecordingTransferStore::open(temp.path(), RecordingTransferStoreConfig::default())?;
    let raw_limit = usize::try_from(crate::protocol::CHUNK_RAW_BYTES)
        .map_err(|_| RecordingTransferStoreError::InvalidState(TempStoreError::InvalidSize))?;
    let payload = deterministic_payload(raw_limit + 17);
    let media_path = store.start_recording_at("recording-1", 1_000)?;
    std::fs::write(&media_path, &payload)
        .map_err(|_| RecordingTransferStoreError::Disk(TempStoreDiskError::Io))?;

    let pending =
        store.finalize_recording_at("recording-1", recording_metadata("recording-1.mp4"), 2_000)?;
    let first = store.recording_chunk_at("recording-1", 0, 3_000)?.ok_or(
        RecordingTransferStoreError::Transfer(crate::transfer::error::TransferError::EmptyTransfer),
    )?;
    let second = store.recording_chunk_at("recording-1", 1, 3_000)?.ok_or(
        RecordingTransferStoreError::Transfer(crate::transfer::error::TransferError::EmptyTransfer),
    )?;

    assert_eq!(pending.total_bytes, crate::protocol::CHUNK_RAW_BYTES + 17);
    assert_eq!(pending.chunk_count, 2);
    assert_eq!(first.raw_bytes, crate::protocol::CHUNK_RAW_BYTES);
    assert_eq!(first.chunk_sha256, sha256_hex(&payload[..raw_limit]));
    assert_eq!(second.raw_bytes, 17);
    assert_eq!(second.chunk_sha256, sha256_hex(&payload[raw_limit..]));
    assert_eq!(
        store.pending_recordings_for_reconnect_at(3_000)?[0].next_missing_chunk_index,
        2
    );
    Ok(())
}

#[test]
fn recording_transfer_store_chunk_lookup_deletes_expired_record_and_media()
-> Result<(), RecordingTransferStoreError> {
    let temp = tempfile::tempdir()
        .map_err(|_| RecordingTransferStoreError::Disk(TempStoreDiskError::Io))?;
    let disk = TempMediaDiskStore::open(temp.path()).map_err(RecordingTransferStoreError::Disk)?;
    let mut record = TempMediaRecord::start_writing("recording-1", 1_000)
        .map_err(RecordingTransferStoreError::InvalidState)?;
    record
        .finalize_recording(
            11,
            &sha256_hex(b"hello world"),
            recording_metadata("recording-1.mp4"),
            2_000,
        )
        .map_err(RecordingTransferStoreError::InvalidState)?;
    disk.write_record(&record)
        .map_err(RecordingTransferStoreError::Disk)?;
    std::fs::write(temp.path().join("recording-1.mp4"), b"hello world")
        .map_err(|_| RecordingTransferStoreError::Disk(TempStoreDiskError::Io))?;
    let store = RecordingTransferStore::open(
        temp.path(),
        RecordingTransferStoreConfig {
            writing_expiry: 500,
            finalized_expiry: 500,
            rejected_retention: 60_000,
        },
    )?;

    assert_eq!(store.recording_chunk_at("recording-1", 0, 3_000)?, None);
    assert!(!temp.path().join("recording-1.json").exists());
    assert!(!temp.path().join("recording-1.mp4").exists());
    Ok(())
}

#[test]
fn recording_transfer_store_reconnect_cleanup_deletes_expired_media()
-> Result<(), RecordingTransferStoreError> {
    let temp = tempfile::tempdir()
        .map_err(|_| RecordingTransferStoreError::Disk(TempStoreDiskError::Io))?;
    let disk = TempMediaDiskStore::open(temp.path()).map_err(RecordingTransferStoreError::Disk)?;
    let expired = TempMediaRecord::start_writing("recording-1", 1_000)
        .map_err(RecordingTransferStoreError::InvalidState)?;
    disk.write_record(&expired)
        .map_err(RecordingTransferStoreError::Disk)?;
    std::fs::write(temp.path().join("recording-1.mp4"), b"incomplete")
        .map_err(|_| RecordingTransferStoreError::Disk(TempStoreDiskError::Io))?;
    let store = RecordingTransferStore::open(
        temp.path(),
        RecordingTransferStoreConfig {
            writing_expiry: 500,
            finalized_expiry: 60_000,
            rejected_retention: 60_000,
        },
    )?;

    assert_eq!(store.pending_recordings_for_reconnect_at(3_000)?.len(), 0);
    assert!(!temp.path().join("recording-1.json").exists());
    assert!(!temp.path().join("recording-1.mp4").exists());
    Ok(())
}

#[test]
fn recording_transfer_store_rejects_plaintext_media_without_protection_metadata()
-> Result<(), RecordingTransferStoreError> {
    let temp = tempfile::tempdir()
        .map_err(|_| RecordingTransferStoreError::Disk(TempStoreDiskError::Io))?;
    let disk = TempMediaDiskStore::open(temp.path()).map_err(RecordingTransferStoreError::Disk)?;
    let mut record = TempMediaRecord::start_writing("recording-1", 1_000)
        .map_err(RecordingTransferStoreError::InvalidState)?;
    record
        .finalize_recording(
            11,
            &sha256_hex(b"hello world"),
            recording_metadata("recording-1.mp4"),
            2_000,
        )
        .map_err(RecordingTransferStoreError::InvalidState)?;
    disk.write_record(&record)
        .map_err(RecordingTransferStoreError::Disk)?;
    std::fs::write(temp.path().join("recording-1.mp4"), b"hello world")
        .map_err(|_| RecordingTransferStoreError::Disk(TempStoreDiskError::Io))?;
    let store = RecordingTransferStore::open(temp.path(), RecordingTransferStoreConfig::default())?;

    assert_eq!(
        store.recording_chunk_at("recording-1", 0, 3_000),
        Err(RecordingTransferStoreError::Transfer(
            crate::transfer::error::TransferError::ProtectedMedia
        ))
    );
    Ok(())
}

#[test]
fn recording_transfer_store_protects_finalized_media_and_reads_plaintext_chunks()
-> Result<(), RecordingTransferStoreError> {
    let temp = tempfile::tempdir()
        .map_err(|_| RecordingTransferStoreError::Disk(TempStoreDiskError::Io))?;
    let store = RecordingTransferStore::open(temp.path(), RecordingTransferStoreConfig::default())?;
    let media_path = store.start_recording_at("recording-1", 1_000)?;
    std::fs::write(&media_path, b"hello world")
        .map_err(|_| RecordingTransferStoreError::Disk(TempStoreDiskError::Io))?;

    let pending =
        store.finalize_recording_at("recording-1", recording_metadata("recording-1.mp4"), 2_000)?;

    assert_eq!(pending.total_bytes, 11);
    assert_eq!(pending.sha256, sha256_hex(b"hello world"));
    assert_ne!(
        read_test_file(&media_path)
            .map_err(|_| RecordingTransferStoreError::Disk(TempStoreDiskError::Io))?,
        b"hello world"
    );
    let chunk = store.recording_chunk_at("recording-1", 0, 3_000)?.ok_or(
        RecordingTransferStoreError::Transfer(crate::transfer::error::TransferError::EmptyTransfer),
    )?;
    assert_eq!(chunk.chunk_sha256, sha256_hex(b"hello world"));
    assert_eq!(chunk.base64, "aGVsbG8gd29ybGQ=");
    Ok(())
}

#[test]
fn recording_transfer_store_rejects_tampered_protected_media()
-> Result<(), RecordingTransferStoreError> {
    let temp = tempfile::tempdir()
        .map_err(|_| RecordingTransferStoreError::Disk(TempStoreDiskError::Io))?;
    let store = RecordingTransferStore::open(temp.path(), RecordingTransferStoreConfig::default())?;
    let media_path = store.start_recording_at("recording-1", 1_000)?;
    std::fs::write(&media_path, b"hello world")
        .map_err(|_| RecordingTransferStoreError::Disk(TempStoreDiskError::Io))?;
    store.finalize_recording_at("recording-1", recording_metadata("recording-1.mp4"), 2_000)?;
    tamper_last_byte(&media_path)
        .map_err(|_| RecordingTransferStoreError::Disk(TempStoreDiskError::Io))?;

    assert_eq!(
        store.recording_chunk_at("recording-1", 0, 3_000),
        Err(RecordingTransferStoreError::Transfer(
            crate::transfer::error::TransferError::ProtectedMedia
        ))
    );
    Ok(())
}

#[test]
fn recording_transfer_store_rejects_metadata_downgraded_protected_media()
-> Result<(), RecordingTransferStoreError> {
    let temp = tempfile::tempdir()
        .map_err(|_| RecordingTransferStoreError::Disk(TempStoreDiskError::Io))?;
    let store = RecordingTransferStore::open(temp.path(), RecordingTransferStoreConfig::default())?;
    let media_path = store.start_recording_at("recording-1", 1_000)?;
    std::fs::write(&media_path, b"hello world")
        .map_err(|_| RecordingTransferStoreError::Disk(TempStoreDiskError::Io))?;
    store.finalize_recording_at("recording-1", recording_metadata("recording-1.mp4"), 2_000)?;
    write_legacy_finalized_recording_metadata(temp.path(), "recording-1")
        .map_err(|_| RecordingTransferStoreError::Disk(TempStoreDiskError::Io))?;

    assert_eq!(
        store.recording_chunk_at("recording-1", 0, 3_000),
        Err(RecordingTransferStoreError::Transfer(
            crate::transfer::error::TransferError::ProtectedMedia
        ))
    );
    Ok(())
}

#[test]
fn recording_transfer_store_ack_deletes_record_and_media() -> Result<(), RecordingTransferStoreError>
{
    let temp = tempfile::tempdir()
        .map_err(|_| RecordingTransferStoreError::Disk(TempStoreDiskError::Io))?;
    let disk = TempMediaDiskStore::open(temp.path()).map_err(RecordingTransferStoreError::Disk)?;
    let mut record = TempMediaRecord::start_writing("recording-1", 1_000)
        .map_err(RecordingTransferStoreError::InvalidState)?;
    record
        .finalize_recording(
            11,
            &sha256_hex(b"hello world"),
            recording_metadata("recording-1.mp4"),
            2_000,
        )
        .map_err(RecordingTransferStoreError::InvalidState)?;
    disk.write_record(&record)
        .map_err(RecordingTransferStoreError::Disk)?;
    std::fs::write(temp.path().join("recording-1.mp4"), b"hello world")
        .map_err(|_| RecordingTransferStoreError::Disk(TempStoreDiskError::Io))?;
    let store = RecordingTransferStore::open(temp.path(), RecordingTransferStoreConfig::default())?;

    assert!(store.ack_recording_at("recording-1", 3_000)?);
    assert!(!temp.path().join("recording-1.json").exists());
    assert!(!temp.path().join("recording-1.mp4").exists());
    Ok(())
}

#[test]
fn recording_transfer_store_reject_retains_diagnostics_when_requested()
-> Result<(), RecordingTransferStoreError> {
    let temp = tempfile::tempdir()
        .map_err(|_| RecordingTransferStoreError::Disk(TempStoreDiskError::Io))?;
    let disk = TempMediaDiskStore::open(temp.path()).map_err(RecordingTransferStoreError::Disk)?;
    let mut record = TempMediaRecord::start_writing("recording-1", 1_000)
        .map_err(RecordingTransferStoreError::InvalidState)?;
    record
        .finalize_recording(
            11,
            &sha256_hex(b"hello world"),
            recording_metadata("recording-1.mp4"),
            2_000,
        )
        .map_err(RecordingTransferStoreError::InvalidState)?;
    disk.write_record(&record)
        .map_err(RecordingTransferStoreError::Disk)?;
    std::fs::write(temp.path().join("recording-1.mp4"), b"hello world")
        .map_err(|_| RecordingTransferStoreError::Disk(TempStoreDiskError::Io))?;
    let store = RecordingTransferStore::open(temp.path(), RecordingTransferStoreConfig::default())?;

    assert!(store.reject_recording_at("recording-1", true, 3_000)?);
    assert!(temp.path().join("recording-1.json").exists());
    assert!(temp.path().join("recording-1.mp4").exists());
    assert_eq!(
        disk.read_record("recording-1")
            .map_err(RecordingTransferStoreError::Disk)?
            .map(|record| record.state),
        Some(TempMediaState::Rejected)
    );
    Ok(())
}

#[test]
fn recording_transfer_store_reject_retains_protected_diagnostics()
-> Result<(), RecordingTransferStoreError> {
    let temp = tempfile::tempdir()
        .map_err(|_| RecordingTransferStoreError::Disk(TempStoreDiskError::Io))?;
    let store = RecordingTransferStore::open(temp.path(), RecordingTransferStoreConfig::default())?;
    let media_path = store.start_recording_at("recording-1", 1_000)?;
    std::fs::write(&media_path, b"hello world")
        .map_err(|_| RecordingTransferStoreError::Disk(TempStoreDiskError::Io))?;
    store.finalize_recording_at("recording-1", recording_metadata("recording-1.mp4"), 2_000)?;

    assert!(store.reject_recording_at("recording-1", true, 3_000)?);
    assert_ne!(
        read_test_file(&media_path)
            .map_err(|_| RecordingTransferStoreError::Disk(TempStoreDiskError::Io))?,
        b"hello world"
    );
    Ok(())
}

#[test]
fn recording_transfer_store_summarizes_startup_recovery_scan()
-> Result<(), RecordingTransferStoreError> {
    let temp = tempfile::tempdir()
        .map_err(|_| RecordingTransferStoreError::Disk(TempStoreDiskError::Io))?;
    let disk = TempMediaDiskStore::open(temp.path()).map_err(RecordingTransferStoreError::Disk)?;
    let mut pending = TempMediaRecord::start_writing("recording-1", 1_000)
        .map_err(RecordingTransferStoreError::InvalidState)?;
    pending
        .finalize_recording(
            11,
            &sha256_hex(b"hello world"),
            recording_metadata("recording-1.mp4"),
            2_000,
        )
        .map_err(RecordingTransferStoreError::InvalidState)?;
    let writing = TempMediaRecord::start_writing("recording-2", 2_500)
        .map_err(RecordingTransferStoreError::InvalidState)?;
    let expired = TempMediaRecord::start_writing("recording-3", 1_000)
        .map_err(RecordingTransferStoreError::InvalidState)?;
    disk.write_record(&pending)
        .map_err(RecordingTransferStoreError::Disk)?;
    disk.write_record(&writing)
        .map_err(RecordingTransferStoreError::Disk)?;
    disk.write_record(&expired)
        .map_err(RecordingTransferStoreError::Disk)?;
    std::fs::write(temp.path().join("recording-3.mp4"), b"incomplete")
        .map_err(|_| RecordingTransferStoreError::Disk(TempStoreDiskError::Io))?;
    let store = RecordingTransferStore::open(
        temp.path(),
        RecordingTransferStoreConfig {
            writing_expiry: 500,
            finalized_expiry: 60_000,
            rejected_retention: 60_000,
        },
    )?;

    let summary = store.startup_recovery_summary_at(3_000)?;

    assert_eq!(summary.pending_recordings, 1);
    assert_eq!(summary.keep_writing, 0);
    assert_eq!(summary.deleted, 2);
    assert!(temp.path().join("recording-1.json").exists());
    assert!(!temp.path().join("recording-2.json").exists());
    assert!(!temp.path().join("recording-3.json").exists());
    assert!(!temp.path().join("recording-3.mp4").exists());
    Ok(())
}

#[test]
fn recording_transfer_store_periodic_cleanup_deletes_expired_records()
-> Result<(), RecordingTransferStoreError> {
    let temp = tempfile::tempdir()
        .map_err(|_| RecordingTransferStoreError::Disk(TempStoreDiskError::Io))?;
    let disk = TempMediaDiskStore::open(temp.path()).map_err(RecordingTransferStoreError::Disk)?;
    let mut pending = TempMediaRecord::start_writing("recording-1", 1_000)
        .map_err(RecordingTransferStoreError::InvalidState)?;
    pending
        .finalize_recording(
            11,
            &sha256_hex(b"hello world"),
            recording_metadata("recording-1.mp4"),
            2_000,
        )
        .map_err(RecordingTransferStoreError::InvalidState)?;
    let expired = TempMediaRecord::start_writing("recording-2", 1_000)
        .map_err(RecordingTransferStoreError::InvalidState)?;
    disk.write_record(&pending)
        .map_err(RecordingTransferStoreError::Disk)?;
    disk.write_record(&expired)
        .map_err(RecordingTransferStoreError::Disk)?;
    std::fs::write(temp.path().join("recording-2.mp4"), b"incomplete")
        .map_err(|_| RecordingTransferStoreError::Disk(TempStoreDiskError::Io))?;
    let store = RecordingTransferStore::open(
        temp.path(),
        RecordingTransferStoreConfig {
            writing_expiry: 500,
            finalized_expiry: 60_000,
            rejected_retention: 60_000,
        },
    )?;

    let summary = store.cleanup_expired_at(3_000)?;

    assert_eq!(summary.pending_recordings, 1);
    assert_eq!(summary.deleted, 1);
    assert!(temp.path().join("recording-1.json").exists());
    assert!(!temp.path().join("recording-2.json").exists());
    assert!(!temp.path().join("recording-2.mp4").exists());
    Ok(())
}

#[test]
fn recording_transfer_cleanup_scan_classifies_bounded_mixed_records()
-> Result<(), RecordingTransferStoreError> {
    let temp = tempfile::tempdir()
        .map_err(|_| RecordingTransferStoreError::Disk(TempStoreDiskError::Io))?;
    let disk = TempMediaDiskStore::open(temp.path()).map_err(RecordingTransferStoreError::Disk)?;
    let mut pending = TempMediaRecord::start_writing("recording-pending", 2_000)
        .map_err(RecordingTransferStoreError::InvalidState)?;
    pending
        .finalize_recording(
            11,
            &sha256_hex(b"hello world"),
            recording_metadata("recording-pending.mp4"),
            2_700,
        )
        .map_err(RecordingTransferStoreError::InvalidState)?;
    let expired = TempMediaRecord::start_writing("recording-expired", 1_000)
        .map_err(RecordingTransferStoreError::InvalidState)?;
    let writing = TempMediaRecord::start_writing("recording-writing", 2_800)
        .map_err(RecordingTransferStoreError::InvalidState)?;
    let mut rejected = TempMediaRecord::start_writing("recording-rejected", 2_000)
        .map_err(RecordingTransferStoreError::InvalidState)?;
    rejected
        .finalize_recording(
            11,
            &sha256_hex(b"hello world"),
            recording_metadata("recording-rejected.mp4"),
            2_100,
        )
        .map_err(RecordingTransferStoreError::InvalidState)?;
    rejected
        .reject(2_200, Some(3_200))
        .map_err(RecordingTransferStoreError::InvalidState)?;
    for record in [pending, expired, writing, rejected] {
        disk.write_record(&record)
            .map_err(RecordingTransferStoreError::Disk)?;
    }
    std::fs::write(temp.path().join("recording-expired.mp4"), b"incomplete")
        .map_err(|_| RecordingTransferStoreError::Disk(TempStoreDiskError::Io))?;
    let store = RecordingTransferStore::open(
        temp.path(),
        RecordingTransferStoreConfig {
            writing_expiry: 500,
            finalized_expiry: 500,
            rejected_retention: 500,
        },
    )?;

    let summary = store.cleanup_expired_at(3_000)?;

    assert_eq!(summary.pending_recordings, 1);
    assert_eq!(summary.keep_writing, 1);
    assert_eq!(summary.retained_rejected, 1);
    assert_eq!(summary.deleted, 1);
    assert!(temp.path().join("recording-pending.json").exists());
    assert!(!temp.path().join("recording-expired.json").exists());
    assert!(temp.path().join("recording-writing.json").exists());
    assert!(temp.path().join("recording-rejected.json").exists());
    Ok(())
}

#[test]
fn disk_temp_store_rejects_untrusted_metadata_shapes() -> Result<(), TempStoreDiskError> {
    let temp = tempfile::tempdir().map_err(|_| TempStoreDiskError::Io)?;
    let store = TempMediaDiskStore::open(temp.path())?;
    std::fs::write(
        temp.path().join("bad-finalized.json"),
        br#"{
            "media_id":"bad-finalized",
            "state":"Finalized",
            "created_at_ms":1000,
            "updated_at_ms":2000,
            "total_bytes":1024,
            "sha256":"not-a-sha",
            "retain_rejected_until_ms":null
        }"#,
    )
    .map_err(|_| TempStoreDiskError::Io)?;

    assert_eq!(
        store.scan_recovery(3_000, 500, 60_000),
        Err(TempStoreDiskError::InvalidMedia(
            TempStoreError::InvalidSha256
        ))
    );

    let temp = tempfile::tempdir().map_err(|_| TempStoreDiskError::Io)?;
    let store = TempMediaDiskStore::open(temp.path())?;
    std::fs::write(
        temp.path().join("bad-writing.json"),
        br#"{
            "media_id":"bad-writing",
            "state":"Writing",
            "created_at_ms":1000,
            "updated_at_ms":1000,
            "total_bytes":1024,
            "sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "retain_rejected_until_ms":null
        }"#,
    )
    .map_err(|_| TempStoreDiskError::Io)?;

    assert_eq!(
        store.scan_recovery(1_100, 500, 60_000),
        Err(TempStoreDiskError::InvalidMedia(
            TempStoreError::InvalidTransition
        ))
    );
    Ok(())
}

fn deterministic_payload(len: usize) -> Vec<u8> {
    (0..len)
        .map(|index| u8::try_from(index % 251).unwrap_or(0))
        .collect()
}
