use crate::{
    protocol::trajectory::{
        CursorTrajectoryBlock, CursorTrajectoryClock, CursorTrajectoryClockSource,
        CursorTrajectoryRecord, CursorTrajectorySegment, CursorTrajectorySegmentReason,
        CursorTrajectoryStreamValidator, decode_trajectory_block,
    },
    telemetry::trajectory_producer::CursorTrajectoryProducer,
};
use serde_json::Value;

type ProofError = Box<dyn std::error::Error>;

fn proof<T>(
    value: Result<T, crate::protocol::trajectory::TrajectoryContractError>,
) -> Result<T, ProofError> {
    value.map_err(|error| format!("{error:?}").into())
}

fn initial() -> CursorTrajectorySegment {
    CursorTrajectorySegment {
        segment_id: 0,
        active_time_us: 0,
        width: 1280,
        height: 800,
        clock: CursorTrajectoryClock {
            source: CursorTrajectoryClockSource::DomEventTime,
            timestamp_quantum_us: None,
            resolution_bound_us: None,
            mapping_uncertainty_us: None,
        },
        reason: CursorTrajectorySegmentReason::Start,
    }
}

#[test]
fn shared_normalization_vectors_preserve_intervals_without_inventing_coordinates()
-> Result<(), ProofError> {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../contracts/native/fixtures/cursor-trajectory-v1.json"
    ))?;
    for case in fixture["normalizationCases"].as_array().ok_or("cases")? {
        let records: Vec<CursorTrajectoryRecord> =
            serde_json::from_value(case.get("input").unwrap_or(&case["records"]).clone())?;
        let start = match records.first() {
            Some(CursorTrajectoryRecord::Segment(segment)) => segment.clone(),
            _ => initial(),
        };
        let mut producer = proof(CursorTrajectoryProducer::new(
            "recording-proof".to_owned(),
            "a".repeat(32),
            start,
        ))?;
        let mut bytes = Vec::new();
        for (index, record) in records.iter().enumerate() {
            match record {
                CursorTrajectoryRecord::Segment(_) if index == 0 => {}
                CursorTrajectoryRecord::Segment(segment) => {
                    bytes.extend(proof(producer.begin_segment(
                        segment.active_time_us,
                        (segment.width, segment.height),
                        segment.clock,
                        segment.reason,
                    ))?);
                }
                CursorTrajectoryRecord::Sample(sample) => {
                    bytes.extend(proof(producer.observe(sample.clone()))?);
                }
                CursorTrajectoryRecord::Boundary(boundary) => bytes.extend(proof(
                    producer.boundary(boundary.active_time_us, boundary.reason),
                )?),
                CursorTrajectoryRecord::Gap(gap) => bytes.extend(proof(producer.gap(gap.clone()))?),
            }
            assert!(producer.pending_observations() <= 512);
        }
        bytes.extend(proof(producer.finish())?);
        let mut validator = CursorTrajectoryStreamValidator::default();
        let blocks: Vec<CursorTrajectoryBlock> = bytes
            .iter()
            .map(|block| proof(decode_trajectory_block(block)))
            .collect::<Result<_, _>>()?;
        let mut segments: Vec<Vec<u64>> = Vec::new();
        let mut retained = Vec::new();
        for block in &blocks {
            proof(validator.accept(block))?;
            for record in &block.records {
                match record {
                    CursorTrajectoryRecord::Segment(_) => segments.push(Vec::new()),
                    CursorTrajectoryRecord::Sample(sample) => {
                        segments
                            .last_mut()
                            .ok_or("segment")?
                            .push(sample.observation_seq);
                        retained.push(sample.observation_seq);
                        if !sample.observed {
                            assert_eq!((sample.x_q8, sample.y_q8), (None, None));
                        }
                    }
                    _ => {}
                }
            }
        }
        assert_eq!(
            serde_json::to_value(segments)?,
            case["expectedSegmentObservationSeq"],
            "{}",
            case["id"]
        );
        assert_eq!(
            serde_json::to_value(retained)?,
            case["expectedRetainedObservationSeq"],
            "{}",
            case["id"]
        );
        let incomplete = case["expectedFinalState"] == "incomplete";
        assert_eq!(producer.incomplete(), incomplete);
        assert_eq!(validator.incomplete(), incomplete);
    }
    Ok(())
}
