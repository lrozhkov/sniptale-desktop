use super::{ProofError, RECORDING, TRAJECTORY, create, directory, fixture};
use crate::{
    protocol::trajectory::{CursorTrajectoryReceipt, CursorTrajectoryState},
    transfer::trajectory_store::{TrajectoryArchive, TrajectoryStoreError, TrajectoryVideoBinding},
};

#[test]
fn a_failed_cursor_erasure_stays_unreadable_and_startup_retries_the_explicit_request()
-> Result<(), ProofError> {
    use crate::transfer::trajectory_catalog::TrajectoryCatalog;
    let root = tempfile::tempdir()?;
    let (_, bytes) = fixture()?;
    let mut writer = create(root.path())?;
    writer.append(&bytes)?;
    writer.finalize(false)?;
    let archive = writer.into_archive();
    let path = directory(root.path());
    std::fs::create_dir(path.join("head.pending"))?;
    assert!(archive.erase().is_err());
    assert!(matches!(
        TrajectoryArchive::read(root.path(), RECORDING, TRAJECTORY),
        Err(TrajectoryStoreError::Retired)
    ));
    let catalog = TrajectoryCatalog::open(root.path().to_owned());
    assert!(catalog.lookup(RECORDING, TRAJECTORY)?.is_none());
    assert!(catalog.recover_interrupted().is_err());
    std::fs::remove_dir(path.join("head.pending"))?;
    catalog.recover_interrupted()?;
    for name in ["head.json", "blocks.bin", "index.bin"] {
        assert!(!path.join(name).exists(), "{name}");
    }
    assert!(create(root.path()).is_err());
    Ok(())
}

fn receipt(
    descriptor: &crate::protocol::trajectory::CursorTrajectoryDescriptor,
) -> CursorTrajectoryReceipt {
    CursorTrajectoryReceipt {
        version: 1,
        durable: true,
        format: descriptor.format,
        state: descriptor.state,
        total_blocks: descriptor.total_blocks,
        total_records: descriptor.total_records,
        total_samples: descriptor.total_samples,
        total_bytes: descriptor.total_bytes,
        stream_sha256: descriptor.stream_sha256.clone(),
    }
}

#[test]
fn video_receipt_keeps_sidecar_and_each_attachment_receipt_field_is_required()
-> Result<(), ProofError> {
    let root = tempfile::tempdir()?;
    let (_, bytes) = fixture()?;
    let mut writer = create(root.path())?;
    writer.append(&bytes)?;
    let descriptor = writer.finalize(false)?;
    let mut archive = writer.into_archive();
    let video = TrajectoryVideoBinding {
        sha256: "a".repeat(64),
        bytes: 50000,
        duration_ms: 100,
        width: 1280,
        height: 800,
    };
    archive.bind_video(video.clone())?;
    let mut other = video.clone();
    other.width = 1281;
    assert_eq!(
        archive.bind_video(other),
        Err(TrajectoryStoreError::Conflict)
    );
    assert!(
        archive
            .accept_video_receipt(video.bytes + 1, &video.sha256)
            .is_err()
    );
    assert!(
        archive
            .accept_video_receipt(video.bytes, &"b".repeat(64))
            .is_err()
    );
    archive.accept_video_receipt(video.bytes, &video.sha256)?;
    assert!(!archive.both_receipts_accepted());
    drop(archive);
    let mut archive = TrajectoryArchive::open(root.path(), RECORDING, TRAJECTORY)?;
    assert_eq!(archive.read_block(0)?, bytes);
    let valid = receipt(&descriptor);
    for index in 0..9 {
        let mut wrong = valid.clone();
        match index {
            0 => wrong.version = 2,
            1 => wrong.durable = false,
            2 => wrong.state = CursorTrajectoryState::Incomplete,
            3 => wrong.total_blocks += 1,
            4 => wrong.total_records += 1,
            5 => wrong.total_samples += 1,
            6 => wrong.total_bytes += 1,
            7 => wrong.stream_sha256 = "b".repeat(64),
            _ => wrong.stream_sha256 = wrong.stream_sha256.to_uppercase(),
        }
        assert_eq!(
            archive.accept_attachment_receipt(&wrong),
            Err(TrajectoryStoreError::Invalid)
        );
        assert!(!archive.both_receipts_accepted());
    }
    archive.accept_attachment_receipt(&valid)?;
    archive.accept_attachment_receipt(&valid)?;
    assert!(archive.both_receipts_accepted());
    assert_eq!(archive.descriptor(), &descriptor);
    drop(archive);
    assert!(TrajectoryArchive::open(root.path(), RECORDING, TRAJECTORY)?.both_receipts_accepted());
    Ok(())
}

#[test]
fn unfinished_history_cannot_be_acknowledged_and_retirement_fences_late_append()
-> Result<(), ProofError> {
    let root = tempfile::tempdir()?;
    let (_, bytes) = fixture()?;
    let mut writer = create(root.path())?;
    writer.append(&bytes)?;
    let mut archive = writer.into_archive();
    let pending = receipt(archive.descriptor());
    assert!(archive.accept_attachment_receipt(&pending).is_err());
    archive.erase()?;
    assert!(!directory(root.path()).join("head.json").exists());
    assert!(!directory(root.path()).join("blocks.bin").exists());
    assert!(TrajectoryArchive::open(root.path(), RECORDING, TRAJECTORY).is_err());
    assert!(create(root.path()).is_err());

    let other_root = tempfile::tempdir()?;
    let mut writer = create(other_root.path())?;
    writer.retire()?;
    assert_eq!(writer.append(&bytes), Err(TrajectoryStoreError::Retired));
    assert!(writer.finalize(false).is_err());
    drop(writer);
    assert!(matches!(
        TrajectoryArchive::open(other_root.path(), RECORDING, TRAJECTORY),
        Err(TrajectoryStoreError::Retired)
    ));
    Ok(())
}

#[test]
fn video_ack_recovery_requires_the_exact_assignment_and_retains_history_until_its_receipt()
-> Result<(), ProofError> {
    use crate::transfer::{
        browser_delivery::{BrowserDeliveryStore, BrowserMediaKind},
        trajectory_catalog::TrajectoryCatalog,
    };
    let root = tempfile::tempdir()?;
    let assignments = BrowserDeliveryStore::open(root.path().join("assignments"));
    let history = root.path().join("history");
    let catalog = TrajectoryCatalog::open(history.clone());
    let (_, bytes) = fixture()?;
    let mut writer = create(&history)?;
    writer.append(&bytes)?;
    let descriptor = writer.finalize(false)?;
    let mut archive = writer.into_archive();
    archive.bind_video(TrajectoryVideoBinding {
        sha256: "a".repeat(64),
        bytes: 50000,
        duration_ms: 100,
        width: 1280,
        height: 800,
    })?;
    archive.accept_video_receipt(50000, &"a".repeat(64))?;
    drop(archive);
    assert!(
        catalog
            .pending_after_video_ack(&"a".repeat(64), &assignments)?
            .is_empty()
    );
    let route = crate::protocol::DeliveryRoute::DesktopDelivery {
        asset_job_id: "job-fixture".to_owned(),
        target_id: Some("a".repeat(64)),
        ask_after_capture: false,
    };
    assignments
        .select(
            RECORDING,
            BrowserMediaKind::Recording,
            &route,
            &"a".repeat(64),
        )
        .map_err(|error| format!("{error:?}"))?;
    assert!(
        catalog
            .authorized_read(RECORDING, TRAJECTORY, &"b".repeat(64), &assignments)
            .is_err()
    );
    assert!(
        catalog
            .pending_after_video_ack(&"b".repeat(64), &assignments)?
            .is_empty()
    );
    let pending = catalog.pending_after_video_ack(&"a".repeat(64), &assignments)?;
    assert_eq!(pending.len(), 1);
    assert!(pending[0].video_receipt_accepted);
    assert!(!pending[0].attachment_receipt_accepted);
    let mut archive =
        catalog.authorized_mutation(RECORDING, TRAJECTORY, &"a".repeat(64), &assignments)?;
    assert_eq!(archive.read_block(0)?, bytes);
    archive.accept_attachment_receipt(&receipt(&descriptor))?;
    archive.release_after_receipts()?;
    assert!(!directory(&history).join("blocks.bin").exists());
    assert!(!directory(&history).join("index.bin").exists());
    assert!(
        catalog
            .pending_after_video_ack(&"a".repeat(64), &assignments)?
            .is_empty()
    );
    assert!(create(&history).is_err());
    Ok(())
}

#[test]
fn a_gap_derived_from_bytes_stays_incomplete_across_finalization_reopen_and_recovery()
-> Result<(), ProofError> {
    use crate::protocol::trajectory::{
        CursorTrajectoryGap, CursorTrajectoryGapReason, CursorTrajectoryRecord,
        CursorTrajectorySegmentReason, encode_trajectory_block,
    };
    let root = tempfile::tempdir()?;
    let (mut block, _) = fixture()?;
    let Some(CursorTrajectoryRecord::Segment(mut resume)) = block.records.first().cloned() else {
        return Err("segment".into());
    };
    let Some(CursorTrajectoryRecord::Boundary(mut stop)) = block.records.pop() else {
        return Err("stop".into());
    };
    resume.segment_id = 1;
    resume.active_time_us = block.end_time_us;
    resume.reason = CursorTrajectorySegmentReason::AfterGap;
    stop.segment_id = 1;
    block
        .records
        .push(CursorTrajectoryRecord::Gap(CursorTrajectoryGap {
            segment_id: 0,
            from_time_us: block.end_time_us,
            to_time_us: Some(block.end_time_us),
            first_lost_observation_seq: Some(3),
            last_lost_observation_seq: Some(4),
            reason: CursorTrajectoryGapReason::InputOverflow,
        }));
    let bytes = encode_trajectory_block(&block).map_err(|error| format!("{error:?}"))?;
    let mut writer = create(root.path())?;
    writer.append(&bytes)?;
    block.sequence = 1;
    block.start_time_us = block.end_time_us;
    block.records = vec![
        CursorTrajectoryRecord::Segment(resume),
        CursorTrajectoryRecord::Boundary(stop),
    ];
    writer.append(&encode_trajectory_block(&block).map_err(|error| format!("{error:?}"))?)?;
    let descriptor = writer.finalize(false)?;
    assert_eq!(descriptor.state, CursorTrajectoryState::Incomplete);
    drop(writer);
    let mut archive = TrajectoryArchive::open(root.path(), RECORDING, TRAJECTORY)?;
    archive.recover_interrupted()?;
    assert_eq!(archive.descriptor(), &descriptor);
    let mut forged = receipt(&descriptor);
    forged.state = CursorTrajectoryState::Complete;
    assert_eq!(
        archive.accept_attachment_receipt(&forged),
        Err(TrajectoryStoreError::Invalid)
    );
    assert_eq!(
        archive.descriptor().state,
        CursorTrajectoryState::Incomplete
    );
    archive.accept_attachment_receipt(&receipt(&descriptor))?;
    Ok(())
}
