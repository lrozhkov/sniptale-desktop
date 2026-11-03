use super::{ProofError, create, fixture};
use crate::{
    protocol::trajectory::{
        CursorTrajectoryBoundaryReason, CursorTrajectoryPacker, CursorTrajectoryRecord,
        encode_trajectory_block,
    },
    transfer::hashing::sha256_hex,
};

#[test]
fn manifest_pages_are_bounded_prefix_checked_and_seek_uses_the_index() -> Result<(), ProofError> {
    let root = tempfile::tempdir()?;
    let (template, _) = fixture()?;
    let mut writer = create(root.path())?;
    for index in 0..35_u64 {
        let mut block = template.clone();
        block.sequence = index;
        block.start_time_us += index * 200_000;
        block.end_time_us += index * 200_000;
        let mut next_observation = index * 2;
        for record in &mut block.records {
            match record {
                CursorTrajectoryRecord::Segment(segment) => {
                    segment.segment_id = index;
                    segment.active_time_us += index * 200_000;
                }
                CursorTrajectoryRecord::Sample(sample) => {
                    sample.segment_id = index;
                    sample.active_time_us += index * 200_000;
                    sample.observation_seq = next_observation;
                    next_observation += 1;
                }
                CursorTrajectoryRecord::Boundary(boundary) => {
                    boundary.segment_id = index;
                    boundary.active_time_us += index * 200_000;
                    if index < 34 {
                        boundary.reason = CursorTrajectoryBoundaryReason::Pause;
                    }
                }
                CursorTrajectoryRecord::Gap(_) => {}
            }
        }
        let bytes = encode_trajectory_block(&block).map_err(|error| format!("{error:?}"))?;
        writer.append(&bytes)?;
    }
    writer.finalize(false)?;
    let mut archive = writer.into_archive();
    let first = archive.manifest(0, &sha256_hex(&[]))?;
    assert_eq!(first.len(), 32);
    assert!(archive.manifest(32, &"a".repeat(64)).is_err());
    let second = archive.manifest(32, &first[31].prefix_sha256)?;
    assert_eq!(second.len(), 3);
    assert_eq!(second[0].sequence, 32);
    assert_eq!(second[2].sequence, 34);
    assert_eq!(archive.manifest(35, &second[2].prefix_sha256)?, []);
    assert_eq!(archive.block_at_time(0)?.ok_or("entry")?.sequence, 0);
    assert_eq!(archive.block_at_time(100_000)?.ok_or("entry")?.sequence, 0);
    assert_eq!(archive.block_at_time(100_001)?.ok_or("entry")?.sequence, 1);
    assert!(archive.block_at_time(7_000_000)?.is_none());
    Ok(())
}

#[test]
fn large_block_has_two_bounded_chunks_and_reassembles_exact_canonical_bytes()
-> Result<(), ProofError> {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    let root = tempfile::tempdir()?;
    let (template, _) = fixture()?;
    let mut writer = create(root.path())?;
    let mut packer = CursorTrajectoryPacker::new(template.recording_id, template.trajectory_id)
        .map_err(|error| format!("{error:?}"))?;
    let mut records = vec![template.records[0].clone()];
    for index in 0..400_u64 {
        let mut sample = template.records[1].clone();
        if let CursorTrajectoryRecord::Sample(sample) = &mut sample {
            sample.observation_seq = index;
            sample.active_time_us = index * 1000;
            sample.anchors.clear();
        }
        records.push(sample);
    }
    let mut stop = template.records[3].clone();
    if let CursorTrajectoryRecord::Boundary(boundary) = &mut stop {
        boundary.active_time_us = 399_000;
    }
    records.push(stop);
    let mut blocks = Vec::new();
    for record in records {
        for bytes in packer
            .append(&record)
            .map_err(|error| format!("{error:?}"))?
        {
            writer.append(&bytes)?;
            blocks.push(bytes);
        }
    }
    writer.finalize(false)?;
    let mut archive = writer.into_archive();
    let first = archive.chunk(0, 0)?;
    let second = archive.chunk(0, 1)?;
    assert_eq!(first.chunk_count, 2);
    assert_eq!(first.chunk_raw_bytes, 32768);
    assert_eq!(second.chunk_byte_offset, 32768);
    assert!(second.chunk_raw_bytes <= 32768);
    let mut bytes = STANDARD.decode(first.base64)?;
    bytes.extend(STANDARD.decode(second.base64)?);
    assert_eq!(bytes, blocks[0]);
    assert_eq!(sha256_hex(&bytes), first.block_sha256);
    assert!(archive.chunk(0, 2).is_err());
    Ok(())
}
