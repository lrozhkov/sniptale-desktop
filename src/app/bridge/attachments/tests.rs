use super::*;
use crate::{
    ipc::{
        AgentPipeReadShutdown, BridgeForwardRuntime,
        forward_native_host_stream_duplex_with_runtime, read_payload,
    },
    protocol::{DeliveryRoute, parse_wire_message, trajectory::trajectory_read_grant_value},
    transfer::{hashing::sha256_hex, trajectory_store::TrajectoryWriter},
};
use std::io::Cursor;

type ProofError = Box<dyn std::error::Error>;

struct NoopShutdown;
impl AgentPipeReadShutdown for NoopShutdown {
    fn shutdown_agent_read(&self) {}
}

#[test]
fn resident_pipe_carries_only_a_grant_and_native_host_resolves_the_protected_chunk()
-> Result<(), ProofError> {
    let root = tempfile::tempdir()?;
    let fixture: serde_json::Value = crate::protocol::trajectory::trajectory_fixture()?;
    let vector = &fixture["canonicalVectors"][0];
    let bytes = vector["utf8"].as_str().ok_or("bytes")?.as_bytes();
    let recording = vector["value"]["recordingId"].as_str().ok_or("recording")?;
    let trajectory = vector["value"]["trajectoryId"]
        .as_str()
        .ok_or("trajectory")?;
    let mut writer = TrajectoryWriter::create(
        root.path(),
        recording,
        trajectory,
        DeliveryRoute::DesktopDelivery {
            asset_job_id: "job-fixture".to_owned(),
            target_id: Some("a".repeat(64)),
            ask_after_capture: false,
        },
    )?;
    writer.append(bytes)?;
    writer.finalize(false)?;
    drop(writer);
    for correct_hash in [true, false] {
        let grant = CursorTrajectoryReadGrant {
            protocol_version: 2,
            controller_lease_id: "lease-fixture".to_owned(),
            recording_id: recording.to_owned(),
            trajectory_id: trajectory.to_owned(),
            request_id: "request-fixture".to_owned(),
            sequence: 0,
            chunk_index: 0,
            block_bytes: u32::try_from(bytes.len())?,
            block_sha256: if correct_hash {
                sha256_hex(bytes)
            } else {
                "a".repeat(64)
            },
        };
        let control = trajectory_read_grant_value(grant)?;
        assert!(parse_wire_message(&serde_json::to_vec(&control)?).is_err());
        let mut resident_output = Vec::new();
        crate::app::bridge::session::write_response_batch(&mut resident_output, vec![control])?;
        assert!(!resident_output.windows(6).any(|part| part == b"base64"));
        assert!(
            !resident_output
                .windows(bytes.len())
                .any(|part| part == bytes)
        );
        let reader = NativeTrajectoryAttachmentReader {
            root: Some(root.path().to_owned()),
            events_root: None,
        };
        let mut native_output = Vec::new();
        let forwarded = forward_native_host_stream_duplex_with_runtime(
            &mut Cursor::new(Vec::<u8>::new()),
            &mut native_output,
            Cursor::new(resident_output),
            Vec::<u8>::new(),
            NoopShutdown,
            BridgeForwardRuntime {
                log: None,
                attachment_reader: Box::new(reader),
            },
        );
        if correct_hash {
            forwarded?;
            let payload =
                read_payload(&mut Cursor::new(native_output), 1_048_576)?.ok_or("response")?;
            let message = parse_wire_message(&payload)?;
            let WireMessage::AppRecordingAttachmentChunk(chunk) = message else {
                return Err("chunk".into());
            };
            assert_eq!(chunk.block_sha256, sha256_hex(bytes));
            assert_eq!(chunk.chunk_raw_bytes, 837);
            assert_eq!(chunk.request_id, "request-fixture");
        } else {
            assert!(matches!(forwarded, Err(BridgeForwardError::AttachmentRead)));
            assert_eq!(native_output, Vec::<u8>::new());
        }
    }
    Ok(())
}

#[test]
fn event_source_bytes_stay_outside_the_resident_pipe_and_bad_grants_emit_nothing()
-> Result<(), ProofError> {
    let root = tempfile::tempdir()?;
    let fixture: serde_json::Value = crate::protocol::recording_events::recording_events_fixture()?;
    let vector = &fixture["blocks"][0];
    let bytes = vector["utf8"].as_str().ok_or("bytes")?.as_bytes();
    let recording = vector["value"]["recordingId"].as_str().ok_or("recording")?;
    let trajectory = vector["value"]["eventStreamId"]
        .as_str()
        .ok_or("trajectory")?;
    let mut writer = crate::transfer::events_store::EventsWriter::create(
        root.path(),
        recording,
        trajectory,
        DeliveryRoute::DesktopDelivery {
            asset_job_id: "job-fixture".to_owned(),
            target_id: Some("a".repeat(64)),
            ask_after_capture: false,
        },
    )?;
    writer.append(bytes)?;
    writer.finalize(false)?;
    drop(writer);
    for correct_hash in [true, false] {
        let grant = RecordingEventsReadGrant {
            protocol_version: 2,
            controller_lease_id: "lease-fixture".to_owned(),
            recording_id: recording.to_owned(),
            event_stream_id: trajectory.to_owned(),
            request_id: "request-fixture".to_owned(),
            sequence: 0,
            chunk_index: 0,
            block_bytes: u32::try_from(bytes.len())?,
            block_sha256: if correct_hash {
                sha256_hex(bytes)
            } else {
                "a".repeat(64)
            },
        };
        let control = crate::protocol::recording_events::events_read_grant_value(grant)?;
        assert!(parse_wire_message(&serde_json::to_vec(&control)?).is_err());
        let mut resident_output = Vec::new();
        crate::app::bridge::session::write_response_batch(&mut resident_output, vec![control])?;
        assert!(!resident_output.windows(6).any(|part| part == b"base64"));
        assert!(
            !resident_output
                .windows(bytes.len())
                .any(|part| part == bytes)
        );
        let reader = NativeTrajectoryAttachmentReader {
            root: None,
            events_root: Some(root.path().to_owned()),
        };
        let mut native_output = Vec::new();
        let forwarded = forward_native_host_stream_duplex_with_runtime(
            &mut Cursor::new(Vec::<u8>::new()),
            &mut native_output,
            Cursor::new(resident_output),
            Vec::<u8>::new(),
            NoopShutdown,
            BridgeForwardRuntime {
                log: None,
                attachment_reader: Box::new(reader),
            },
        );
        if correct_hash {
            forwarded?;
            let payload =
                read_payload(&mut Cursor::new(native_output), 1_048_576)?.ok_or("response")?;
            let message = parse_wire_message(&payload)?;
            let WireMessage::AppRecordingEventsChunk(chunk) = message else {
                return Err("chunk".into());
            };
            assert_eq!(chunk.block_sha256, sha256_hex(bytes));
            assert_eq!(usize::try_from(chunk.chunk_raw_bytes)?, bytes.len());
            assert_eq!(chunk.request_id, "request-fixture");
        } else {
            assert!(matches!(forwarded, Err(BridgeForwardError::AttachmentRead)));
            assert_eq!(native_output, Vec::<u8>::new());
        }
    }
    Ok(())
}

fn protected_hour_source(
    root: &std::path::Path,
    fixture: &serde_json::Value,
) -> Result<crate::protocol::trajectory::CursorTrajectoryDescriptor, ProofError> {
    use crate::protocol::trajectory::{
        CursorTrajectoryAnchor, CursorTrajectoryBoundaryReason, CursorTrajectoryButton,
        CursorTrajectoryClock, CursorTrajectoryClockSource, CursorTrajectoryEvent,
        CursorTrajectorySample, CursorTrajectorySegment, CursorTrajectorySegmentReason,
    };
    use crate::telemetry::CursorTrajectoryProducer;
    let number = |key: &str| fixture[key].as_u64().ok_or("fixture number");
    let recording = fixture["recordingId"].as_str().ok_or("recording")?;
    let trajectory = fixture["trajectoryId"].as_str().ok_or("trajectory")?;
    let mut producer = CursorTrajectoryProducer::new(
        recording.to_owned(),
        trajectory.to_owned(),
        CursorTrajectorySegment {
            segment_id: 0,
            active_time_us: 0,
            width: 1280,
            height: 800,
            clock: CursorTrajectoryClock {
                source: CursorTrajectoryClockSource::DomEventTime,
                timestamp_quantum_us: Some(number("clockQuantumUs")?),
                resolution_bound_us: Some(number("clockQuantumUs")?),
                mapping_uncertainty_us: Some(0),
            },
            reason: CursorTrajectorySegmentReason::Start,
        },
    )
    .map_err(|error| format!("{error:?}"))?;
    let mut writer = TrajectoryWriter::create(
        root,
        recording,
        trajectory,
        DeliveryRoute::DesktopDelivery {
            asset_job_id: "protected-hour-proof".to_owned(),
            target_id: Some("a".repeat(64)),
            ask_after_capture: false,
        },
    )?;
    let mut append = |blocks: Vec<Vec<u8>>| -> Result<(), ProofError> {
        for bytes in blocks {
            writer.append(&bytes)?;
        }
        Ok(())
    };
    let count = number("observationCount")?;
    for sequence in 0..count {
        let anchors = if sequence == 0 {
            vec![CursorTrajectoryAnchor::Start]
        } else if sequence + 1 == count {
            vec![CursorTrajectoryAnchor::Stop]
        } else {
            Vec::new()
        };
        append(
            producer
                .observe(CursorTrajectorySample {
                    segment_id: 0,
                    observation_seq: sequence,
                    active_time_us: sequence * number("stepUs")?,
                    x_q8: Some(number("xQ8")?),
                    y_q8: Some(number("baseYQ8")? + sequence % 2 * number("oddYOffsetQ8")?),
                    visible: true,
                    observed: true,
                    event: CursorTrajectoryEvent::Move,
                    button: CursorTrajectoryButton::None,
                    related_observation_seq: None,
                    anchors,
                })
                .map_err(|error| format!("{error:?}"))?,
        )?;
        assert!(producer.pending_observations() <= 512);
    }
    append(
        producer
            .boundary(number("durationUs")?, CursorTrajectoryBoundaryReason::Stop)
            .map_err(|error| format!("{error:?}"))?,
    )?;
    append(producer.finish().map_err(|error| format!("{error:?}"))?)?;
    Ok(writer.finalize(false)?)
}

fn framed_protected_chunk(
    root: &std::path::Path,
    grant: CursorTrajectoryReadGrant,
) -> Result<crate::protocol::trajectory::AppRecordingAttachmentChunkMessage, ProofError> {
    let mut resident_output = Vec::new();
    crate::app::bridge::session::write_response_batch(
        &mut resident_output,
        vec![trajectory_read_grant_value(grant)?],
    )?;
    assert!(resident_output.len() <= 2304);
    assert!(!resident_output.windows(6).any(|part| part == b"base64"));
    let mut native_output = Vec::new();
    forward_native_host_stream_duplex_with_runtime(
        &mut Cursor::new(Vec::<u8>::new()),
        &mut native_output,
        Cursor::new(resident_output),
        Vec::<u8>::new(),
        NoopShutdown,
        BridgeForwardRuntime {
            log: None,
            attachment_reader: Box::new(NativeTrajectoryAttachmentReader {
                root: Some(root.to_owned()),
                events_root: None,
            }),
        },
    )?;
    assert!(native_output.len() <= 65536);
    let payload = read_payload(&mut Cursor::new(native_output), 65536)?.ok_or("chunk frame")?;
    let WireMessage::AppRecordingAttachmentChunk(chunk) = parse_wire_message(&payload)? else {
        return Err("public cursor chunk".into());
    };
    Ok(*chunk)
}

#[test]
fn a_full_hour_survives_protected_storage_paged_manifests_and_native_host_framing()
-> Result<(), ProofError> {
    use crate::transfer::trajectory_store::TrajectoryArchive;
    use sha2::{Digest, Sha256};
    let root = tempfile::tempdir()?;
    let fixture: serde_json::Value = crate::protocol::trajectory::trajectory_fixture()?;
    let source = &fixture["longTrace"];
    let expected = &source["expected"];
    let recording = source["recordingId"].as_str().ok_or("recording")?;
    let trajectory = source["trajectoryId"].as_str().ok_or("trajectory")?;
    let descriptor = protected_hour_source(root.path(), source)?;
    assert_eq!(descriptor.total_samples, 360_001);
    assert_eq!(descriptor.total_blocks, expected["totalBlocks"]);
    assert_eq!(descriptor.stream_sha256, expected["streamSha256"]);
    let mut archive = TrajectoryArchive::read(root.path(), recording, trajectory)?;
    let mut hash = Sha256::new();
    let mut sequence = 0;
    let mut prefix = sha256_hex(&[]);
    let mut observations = 0_u64;
    while sequence < descriptor.total_blocks {
        let page = archive.manifest(sequence, &prefix)?;
        assert!(!page.is_empty() && page.len() <= 32);
        for entry in page {
            let mut bytes = Vec::new();
            for chunk_index in 0..entry.byte_length.div_ceil(32768) {
                let chunk = framed_protected_chunk(
                    root.path(),
                    CursorTrajectoryReadGrant {
                        protocol_version: 2,
                        controller_lease_id: "lease-hour".to_owned(),
                        recording_id: recording.to_owned(),
                        trajectory_id: trajectory.to_owned(),
                        request_id: format!("hour-{}-{chunk_index}", entry.sequence),
                        sequence: entry.sequence,
                        chunk_index: u8::try_from(chunk_index)?,
                        block_bytes: entry.byte_length,
                        block_sha256: entry.block_sha256.clone(),
                    },
                )?;
                let raw = crate::transfer::chunks::decode_chunk_for_test(&chunk.base64)?;
                assert!(raw.len() <= 32768);
                assert_eq!(sha256_hex(&raw), chunk.chunk_sha256);
                assert_eq!(usize::try_from(chunk.chunk_byte_offset)?, bytes.len());
                bytes.extend(raw);
            }
            assert_eq!(bytes.len(), usize::try_from(entry.byte_length)?);
            assert_eq!(sha256_hex(&bytes), entry.block_sha256);
            let block = crate::protocol::trajectory::decode_trajectory_block(&bytes)
                .map_err(|error| format!("{error:?}"))?;
            observations += u64::from(entry.sample_count);
            assert_eq!(block.sequence, sequence);
            hash.update(entry.byte_length.to_le_bytes());
            hash.update(&bytes);
            prefix = format!("{:x}", hash.clone().finalize());
            assert_eq!(prefix, entry.prefix_sha256);
            sequence += 1;
        }
    }
    assert_eq!(observations, descriptor.total_samples);
    assert!(observations > 4096);
    assert_eq!(prefix, expected["streamSha256"]);
    assert_eq!(archive.descriptor(), &descriptor);
    Ok(())
}
