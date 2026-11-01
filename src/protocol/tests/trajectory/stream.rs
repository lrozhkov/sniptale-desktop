use super::fixture;
use crate::protocol::trajectory::{
    CursorTrajectoryBlock, CursorTrajectoryPacker, CursorTrajectoryRecord,
    CursorTrajectoryStreamValidator, TrajectoryContractError, decode_trajectory_block,
};

type ProofError = Box<dyn std::error::Error>;

fn lifecycle_blocks() -> Result<Vec<CursorTrajectoryBlock>, ProofError> {
    let fixtures = fixture()?;
    let case = &fixtures["packingCases"][0];
    let records: Vec<CursorTrajectoryRecord> = serde_json::from_value(case["records"].clone())?;
    let mut packer = CursorTrajectoryPacker::new(
        case["recordingId"].as_str().ok_or("recording")?.to_owned(),
        case["trajectoryId"]
            .as_str()
            .ok_or("trajectory")?
            .to_owned(),
    )
    .map_err(|error| format!("{error:?}"))?;
    let mut blocks = Vec::new();
    for record in records {
        for bytes in packer
            .append(&record)
            .map_err(|error| format!("{error:?}"))?
        {
            blocks.push(decode_trajectory_block(&bytes).map_err(|error| format!("{error:?}"))?);
        }
    }
    Ok(blocks)
}

#[test]
fn rejected_block_does_not_advance_order_or_change_binding() -> Result<(), ProofError> {
    let blocks = lifecycle_blocks()?;
    let mut validator = CursorTrajectoryStreamValidator::default();
    assert!(validator.accept(&blocks[1]).is_err());
    validator
        .accept(&blocks[0])
        .map_err(|error| format!("{error:?}"))?;
    assert!(!validator.incomplete());
    assert!(!validator.stopped());
    assert!(validator.accept(&blocks[0]).is_err());
    let mut wrong = blocks[1].clone();
    wrong.recording_id.push('x');
    assert_eq!(
        validator.accept(&wrong),
        Err(TrajectoryContractError::Identifier)
    );
    let mut wrong = blocks[1].clone();
    wrong.trajectory_id = "a".repeat(32);
    assert_eq!(
        validator.accept(&wrong),
        Err(TrajectoryContractError::Identifier)
    );
    validator
        .accept(&blocks[1])
        .map_err(|error| format!("{error:?}"))?;
    assert!(validator.stopped());
    let mut after_stop = blocks[0].clone();
    after_stop.sequence = 2;
    assert!(validator.accept(&after_stop).is_err());
    Ok(())
}

#[test]
fn continued_block_uses_original_segment_geometry_and_observation_order() -> Result<(), ProofError>
{
    let blocks = lifecycle_blocks()?;
    let mut first = blocks[0].clone();
    first.records.truncate(2);
    first.end_time_us = 0;
    let mut tail = blocks[0].clone();
    tail.sequence = 1;
    tail.records.drain(..2);
    tail.start_time_us = 20000;
    let mut validator = CursorTrajectoryStreamValidator::default();
    validator
        .accept(&first)
        .map_err(|error| format!("{error:?}"))?;
    let mut outside = tail.clone();
    if let CursorTrajectoryRecord::Sample(sample) = &mut outside.records[0] {
        sample.x_q8 = Some(u64::from(u32::MAX) * 256 - 1);
    }
    assert_eq!(
        validator.accept(&outside),
        Err(TrajectoryContractError::Coordinates)
    );
    let mut replay = tail.clone();
    if let CursorTrajectoryRecord::Sample(sample) = &mut replay.records[0] {
        sample.observation_seq = 0;
    }
    assert_eq!(
        validator.accept(&replay),
        Err(TrajectoryContractError::Order)
    );
    validator
        .accept(&tail)
        .map_err(|error| format!("{error:?}"))?;
    let mut final_block = blocks[1].clone();
    final_block.sequence = 2;
    validator
        .accept(&final_block)
        .map_err(|error| format!("{error:?}"))?;
    Ok(())
}
