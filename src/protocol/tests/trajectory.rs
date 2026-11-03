use crate::protocol::trajectory::{
    CursorTrajectoryBlock, CursorTrajectoryPacker, CursorTrajectoryRecord, decode_trajectory_block,
    encode_trajectory_block,
};
use serde_json::Value;
use sha2::{Digest, Sha256};

mod attachments;
mod stream;

fn fixture() -> Result<Value, Box<dyn std::error::Error>> {
    Ok(serde_json::from_str(include_str!(
        "../../../contracts/native/fixtures/cursor-trajectory-v1.json"
    ))?)
}

#[test]
fn canonical_block_bytes_and_length_prefixed_hash_match_both_peers()
-> Result<(), Box<dyn std::error::Error>> {
    let fixtures = fixture()?;
    let vector = &fixtures["canonicalVectors"][0];
    let block: CursorTrajectoryBlock = serde_json::from_value(vector["value"].clone())?;
    let bytes = encode_trajectory_block(&block).map_err(|error| format!("{error:?}"))?;
    assert_eq!(
        std::str::from_utf8(&bytes)?,
        vector["utf8"].as_str().ok_or("canonical bytes")?
    );
    assert_eq!(
        format!("{:x}", Sha256::digest(&bytes)),
        vector["blockSha256"]
    );
    let length = u32::try_from(bytes.len())?;
    let mut hash = Sha256::new();
    hash.update(length.to_le_bytes());
    hash.update(&bytes);
    assert_eq!(format!("{:x}", hash.finalize()), vector["streamSha256"]);
    assert_eq!(
        decode_trajectory_block(&bytes).map_err(|error| format!("{error:?}"))?,
        block
    );
    assert!(decode_trajectory_block(&[bytes.as_slice(), b" "].concat()).is_err());
    Ok(())
}

#[test]
fn utf8_bom_is_rejected_before_cursor_source_admission() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = fixture()?;
    let bytes = fixture["canonicalVectors"][0]["utf8"]
        .as_str()
        .ok_or("bytes")?
        .as_bytes();
    let mut prefixed = vec![0xef, 0xbb, 0xbf];
    prefixed.extend_from_slice(bytes);
    assert!(decode_trajectory_block(&prefixed).is_err());
    Ok(())
}

#[test]
fn explicit_lifecycle_barriers_seal_the_same_immutable_blocks()
-> Result<(), Box<dyn std::error::Error>> {
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
    let mut count = 0_u64;
    let mut hash = Sha256::new();
    for record in &records {
        for bytes in packer
            .append(record)
            .map_err(|error| format!("{error:?}"))?
        {
            let block = decode_trajectory_block(&bytes).map_err(|error| format!("{error:?}"))?;
            assert_eq!(block.sequence, count);
            assert_eq!(block.records.len(), 4);
            assert_eq!(
                format!("{:x}", Sha256::digest(&bytes)),
                case["expected"]["firstManifestEntries"][usize::try_from(count)?]["blockSha256"]
            );
            hash.update(u32::try_from(bytes.len())?.to_le_bytes());
            hash.update(&bytes);
            count += 1;
        }
    }
    assert!(
        packer
            .finish()
            .map_err(|error| format!("{error:?}"))?
            .is_none()
    );
    assert!(
        packer
            .finish()
            .map_err(|error| format!("{error:?}"))?
            .is_none()
    );
    assert!(packer.append(&records[0]).is_err());
    assert_eq!(
        count,
        case["expected"]["totalBlocks"].as_u64().ok_or("count")?
    );
    assert_eq!(
        format!("{:x}", hash.finalize()),
        case["expected"]["streamSha256"]
    );
    Ok(())
}
