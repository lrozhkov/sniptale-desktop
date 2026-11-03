use crate::protocol::{
    parse_wire_message,
    trajectory::{CursorTrajectoryBlock, decode_trajectory_block, encode_trajectory_block},
};
use serde_json::{Value, json};

type ProofError = Box<dyn std::error::Error>;

fn fixture(id: &str, positive: bool) -> Result<Value, ProofError> {
    let text = if positive {
        include_str!("../../../../contracts/native/fixtures/positive/contract-fixtures.json")
    } else {
        include_str!("../../../../contracts/native/fixtures/negative/contract-fixtures.json")
    };
    let fixtures: Value = serde_json::from_str(text)?;
    Ok(fixtures[id].clone())
}

fn parses(value: &Value) -> Result<bool, ProofError> {
    Ok(parse_wire_message(&serde_json::to_vec(value)?).is_ok())
}

#[test]
fn empty_unavailable_prefix_is_valid_but_video_still_requires_positive_bytes()
-> Result<(), ProofError> {
    let mut available = fixture("AppRecordingAttachmentAvailableMessage", true)?;
    let descriptor = &mut available["descriptor"];
    for key in ["totalBlocks", "totalRecords", "totalSamples", "totalBytes"] {
        descriptor[key] = json!(0);
    }
    descriptor["state"] = json!("unavailable");
    descriptor["streamSha256"] =
        json!("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    assert!(parses(&available)?);
    let mut stopped = fixture("AppRecordingStoppedMessage", true)?;
    stopped["cursorTrajectory"] = available["descriptor"].clone();
    assert!(parses(&stopped)?);
    stopped["totalBytes"] = json!(0);
    assert!(!parses(&stopped)?);
    available["descriptor"]["finalized"] = json!(false);
    assert!(!parses(&available)?);
    Ok(())
}

#[test]
fn manifest_limit_prefix_end_and_required_nullable_field_are_checked() -> Result<(), ProofError> {
    let mut page = fixture("AppRecordingAttachmentManifestMessage", true)?;
    let entry = page["entries"][0].clone();
    for count in [32, 33] {
        let mut entries = Vec::new();
        for sequence in 0..count {
            let mut next = entry.clone();
            next["sequence"] = json!(sequence);
            next["startTimeUs"] = json!(sequence * 100_000);
            next["endTimeUs"] = json!((sequence + 1) * 100_000);
            entries.push(next);
        }
        page["entries"] = json!(entries);
        page["descriptor"]["totalBlocks"] = json!(count);
        page["descriptor"]["totalRecords"] = json!(count * 4);
        page["descriptor"]["totalSamples"] = json!(count * 2);
        page["descriptor"]["totalBytes"] = json!(count * 837);
        assert_eq!(parses(&page)?, count == 32);
    }
    let mut page = fixture("AppRecordingAttachmentManifestMessage", true)?;
    page.as_object_mut().ok_or("page")?.remove("nextSequence");
    assert!(!parses(&page)?);
    let mut page = fixture("AppRecordingAttachmentManifestMessage", true)?;
    page["nextSequence"] = json!(1);
    assert!(!parses(&page)?);
    Ok(())
}

#[test]
fn chunk_offsets_sizes_hash_case_and_unknown_fields_are_rejected() -> Result<(), ProofError> {
    let chunk = fixture("AppRecordingAttachmentChunkMessage", true)?;
    for (key, bad) in [
        ("chunkIndex", json!(1)),
        ("chunkCount", json!(2)),
        ("chunkByteOffset", json!(1)),
        ("chunkRawBytes", json!(32769)),
        ("blockBytes", json!(65537)),
        ("blockSha256", json!("A".repeat(64))),
        ("sequence", json!(9_007_199_254_740_992_u64)),
        ("privatePath", json!("private")),
    ] {
        let mut altered = chunk.clone();
        altered[key] = bad;
        assert!(!parses(&altered)?, "{key}");
    }
    let mut chunk = chunk;
    chunk["base64"] = json!("A".repeat(43700));
    assert!(!parses(&chunk)?);
    Ok(())
}

#[test]
fn non_wire_fixture_contracts_reach_their_real_owner_parser() -> Result<(), ProofError> {
    for positive in [true, false] {
        let block: CursorTrajectoryBlock =
            serde_json::from_value(fixture("CursorTrajectoryBlock", positive)?)?;
        let encoded = encode_trajectory_block(&block);
        assert_eq!(encoded.is_ok(), positive);
        if let Ok(bytes) = encoded {
            assert!(decode_trajectory_block(&bytes).is_ok());
        }
        let mut available = fixture("AppRecordingAttachmentAvailableMessage", true)?;
        available["descriptor"] = fixture("CursorTrajectoryDescriptor", positive)?;
        assert_eq!(parses(&available)?, positive);
        let mut ack = fixture("ExtensionRecordingAttachmentAckMessage", true)?;
        ack["receipt"] = fixture("CursorTrajectoryReceipt", positive)?;
        assert_eq!(parses(&ack)?, positive);
        let mut hello = fixture("AppHelloMessage", true)?;
        hello["capabilities"]["attachments"] =
            fixture("NativeTrajectoryAttachmentCapabilities", positive)?;
        assert_eq!(parses(&hello)?, positive);
    }
    Ok(())
}
