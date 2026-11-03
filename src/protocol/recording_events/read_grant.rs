//! Internal authenticated-pipe authorization metadata; never a browser wire command.

use crate::protocol::trajectory::{MAX_SAFE_INTEGER, valid_trajectory_hash, valid_trajectory_id};
use crate::protocol::{ParseError, errors::ParseErrorKind};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct RecordingEventsReadGrant {
    pub(crate) protocol_version: u16,
    pub(crate) controller_lease_id: String,
    pub(crate) recording_id: String,
    pub(crate) event_stream_id: String,
    pub(crate) request_id: String,
    pub(crate) sequence: u64,
    pub(crate) chunk_index: u8,
    pub(crate) block_bytes: u32,
    pub(crate) block_sha256: String,
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "type")]
enum Envelope {
    #[serde(rename = "agent.recording.events.read-grant")]
    Grant(RecordingEventsReadGrant),
}

pub(crate) fn parse_events_read_grant(
    bytes: &[u8],
) -> Result<RecordingEventsReadGrant, ParseError> {
    if bytes.len() > 2048 {
        return Err(invalid());
    }
    let Envelope::Grant(grant) = serde_json::from_slice(bytes).map_err(|_| invalid())?;
    validate(&grant)?;
    Ok(grant)
}

pub(crate) fn events_read_grant_value(
    grant: RecordingEventsReadGrant,
) -> Result<serde_json::Value, ParseError> {
    validate(&grant)?;
    serde_json::to_value(Envelope::Grant(grant)).map_err(|_| invalid())
}

fn validate(grant: &RecordingEventsReadGrant) -> Result<(), ParseError> {
    if grant.protocol_version != 2
        || !super::attachment_validation::id(&grant.controller_lease_id)
        || !super::attachment_validation::id(&grant.recording_id)
        || !super::attachment_validation::id(&grant.request_id)
        || !valid_trajectory_id(&grant.event_stream_id)
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
