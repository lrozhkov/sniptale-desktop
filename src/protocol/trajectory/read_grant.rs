//! Internal authenticated-pipe authorization metadata; never a browser wire command.

use super::{MAX_SAFE_INTEGER, valid_trajectory_hash, valid_trajectory_id};
use crate::protocol::{ParseError, errors::ParseErrorKind};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CursorTrajectoryReadGrant {
    pub(crate) protocol_version: u16,
    pub(crate) controller_lease_id: String,
    pub(crate) recording_id: String,
    pub(crate) trajectory_id: String,
    pub(crate) request_id: String,
    pub(crate) sequence: u64,
    pub(crate) chunk_index: u8,
    pub(crate) block_bytes: u32,
    pub(crate) block_sha256: String,
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "type")]
enum Envelope {
    #[serde(rename = "agent.recording.attachment.read-grant")]
    Grant(CursorTrajectoryReadGrant),
}

pub(crate) fn parse_trajectory_read_grant(
    bytes: &[u8],
) -> Result<CursorTrajectoryReadGrant, ParseError> {
    if bytes.len() > 2048 {
        return Err(invalid());
    }
    let Envelope::Grant(grant) = serde_json::from_slice(bytes).map_err(|_| invalid())?;
    validate(&grant)?;
    Ok(grant)
}

pub(crate) fn trajectory_read_grant_value(
    grant: CursorTrajectoryReadGrant,
) -> Result<serde_json::Value, ParseError> {
    validate(&grant)?;
    serde_json::to_value(Envelope::Grant(grant)).map_err(|_| invalid())
}

fn validate(grant: &CursorTrajectoryReadGrant) -> Result<(), ParseError> {
    if grant.protocol_version != 2
        || !super::validation::valid_id(&grant.controller_lease_id)
        || !super::validation::valid_id(&grant.recording_id)
        || !super::validation::valid_id(&grant.request_id)
        || !valid_trajectory_id(&grant.trajectory_id)
        || !valid_trajectory_hash(&grant.block_sha256)
        || grant.sequence > MAX_SAFE_INTEGER
        || !(1..=65536).contains(&grant.block_bytes)
        || u32::from(grant.chunk_index) >= grant.block_bytes.div_ceil(32768)
    {
        return Err(invalid());
    }
    Ok(())
}

fn invalid() -> ParseError {
    ParseError::without_field(
        ParseErrorKind::MalformedJson,
        "invalid attachment read grant",
    )
}
