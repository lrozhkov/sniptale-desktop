use crate::{
    protocol::trajectory::{
        CursorTrajectoryAnchor, CursorTrajectoryBoundaryReason, CursorTrajectoryButton,
        CursorTrajectoryClock, CursorTrajectoryClockSource, CursorTrajectoryEvent,
        CursorTrajectoryManifestEntry, CursorTrajectoryRecord, CursorTrajectorySample,
        CursorTrajectorySegment, CursorTrajectorySegmentReason, decode_trajectory_block,
    },
    telemetry::trajectory_producer::CursorTrajectoryProducer,
};
use serde_json::Value;
use sha2::{Digest, Sha256};

type ProofError = Box<dyn std::error::Error>;

#[derive(Default)]
struct StreamProof {
    hash: Sha256,
    blocks: u64,
    records: u64,
    samples: u64,
    bytes: u64,
    first_entries: Vec<Value>,
    last_entry: Value,
}

impl StreamProof {
    fn accept(&mut self, bytes: &[u8]) -> Result<(), ProofError> {
        let block = decode_trajectory_block(bytes).map_err(|error| format!("{error:?}"))?;
        assert_eq!(block.sequence, self.blocks);
        assert!(bytes.len() <= 65536);
        assert!(block.records.len() <= 512);
        let mut samples = 0;
        for record in &block.records {
            if let CursorTrajectoryRecord::Sample(sample) = record {
                assert_eq!(sample.observation_seq, self.samples);
                assert_eq!(sample.active_time_us, self.samples * 10000);
                assert_eq!(sample.x_q8, Some(25600));
                assert_eq!(sample.y_q8, Some(25600 + (self.samples % 2) * 1024));
                self.samples += 1;
                samples += 1;
            }
        }
        self.hash.update(u32::try_from(bytes.len())?.to_le_bytes());
        self.hash.update(bytes);
        let entry = serde_json::to_value(CursorTrajectoryManifestEntry {
            sequence: block.sequence,
            start_time_us: block.start_time_us,
            end_time_us: block.end_time_us,
            record_count: u16::try_from(block.records.len())?,
            sample_count: samples,
            byte_length: u32::try_from(bytes.len())?,
            block_sha256: format!("{:x}", Sha256::digest(bytes)),
            prefix_sha256: format!("{:x}", self.hash.clone().finalize()),
        })?;
        if self.blocks < 3 {
            self.first_entries.push(entry.clone());
        }
        self.last_entry = entry;
        self.blocks += 1;
        self.records += u64::try_from(block.records.len())?;
        self.bytes += u64::try_from(bytes.len())?;
        Ok(())
    }

    fn accept_blocks(&mut self, blocks: Vec<Vec<u8>>) -> Result<(), ProofError> {
        for bytes in blocks {
            self.accept(&bytes)?;
        }
        Ok(())
    }
}

fn sample(sequence: u64, generator: &Value) -> Result<CursorTrajectorySample, ProofError> {
    let count = generator["observationCount"].as_u64().ok_or("count")?;
    Ok(CursorTrajectorySample {
        segment_id: 0,
        observation_seq: sequence,
        active_time_us: sequence * generator["stepUs"].as_u64().ok_or("step")?,
        x_q8: Some(generator["xQ8"].as_u64().ok_or("x")?),
        y_q8: Some(
            generator["baseYQ8"].as_u64().ok_or("y")?
                + sequence % 2 * generator["oddYOffsetQ8"].as_u64().ok_or("offset")?,
        ),
        visible: true,
        observed: true,
        event: CursorTrajectoryEvent::Move,
        button: CursorTrajectoryButton::None,
        related_observation_seq: None,
        anchors: if sequence == 0 {
            vec![CursorTrajectoryAnchor::Start]
        } else if sequence + 1 == count {
            vec![CursorTrajectoryAnchor::Stop]
        } else {
            Vec::new()
        },
    })
}

#[test]
fn full_hour_through_rust_producer_matches_frozen_stream() -> Result<(), ProofError> {
    let fixtures: Value = serde_json::from_str(include_str!(
        "../../../contracts/native/fixtures/cursor-trajectory-v1.json"
    ))?;
    let generator = &fixtures["longTrace"];
    let expected = &generator["expected"];
    let quantum = generator["clockQuantumUs"].as_u64().ok_or("quantum")?;
    let mut producer = CursorTrajectoryProducer::new(
        generator["recordingId"]
            .as_str()
            .ok_or("recording")?
            .to_owned(),
        generator["trajectoryId"]
            .as_str()
            .ok_or("trajectory")?
            .to_owned(),
        CursorTrajectorySegment {
            segment_id: 0,
            active_time_us: 0,
            width: 1280,
            height: 800,
            clock: CursorTrajectoryClock {
                source: CursorTrajectoryClockSource::DomEventTime,
                timestamp_quantum_us: Some(quantum),
                resolution_bound_us: Some(quantum),
                mapping_uncertainty_us: Some(0),
            },
            reason: CursorTrajectorySegmentReason::Start,
        },
    )
    .map_err(|error| format!("{error:?}"))?;
    let mut proof = StreamProof::default();
    for sequence in 0..generator["observationCount"].as_u64().ok_or("count")? {
        proof.accept_blocks(
            producer
                .observe(sample(sequence, generator)?)
                .map_err(|error| format!("{error:?}"))?,
        )?;
        assert!(producer.pending_observations() <= 512);
    }
    proof.accept_blocks(
        producer
            .boundary(
                generator["durationUs"].as_u64().ok_or("duration")?,
                CursorTrajectoryBoundaryReason::Stop,
            )
            .map_err(|error| format!("{error:?}"))?,
    )?;
    proof.accept_blocks(producer.finish().map_err(|error| format!("{error:?}"))?)?;
    assert_eq!(
        proof.blocks,
        expected["totalBlocks"].as_u64().ok_or("blocks")?
    );
    assert_eq!(
        proof.records,
        expected["totalRecords"].as_u64().ok_or("records")?
    );
    assert_eq!(
        proof.samples,
        expected["totalSamples"].as_u64().ok_or("samples")?
    );
    assert!(proof.samples > 4096);
    assert_eq!(proof.bytes, expected["totalBytes"].as_u64().ok_or("bytes")?);
    assert_eq!(
        serde_json::to_value(proof.first_entries)?,
        expected["firstManifestEntries"]
    );
    assert_eq!(proof.last_entry, expected["lastBlock"]);
    assert_eq!(
        format!("{:x}", proof.hash.finalize()),
        expected["streamSha256"]
    );
    Ok(())
}
