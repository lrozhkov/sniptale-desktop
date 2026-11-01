//! Independent event-stream descriptor and durable receipt admission.

use super::EventContractError;
use crate::protocol::trajectory::{
    CursorTrajectoryState, MAX_SAFE_INTEGER, valid_trajectory_hash, valid_trajectory_id,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) enum EventFormat {
    #[serde(rename = "recording-events-v1")]
    V1,
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) enum EventAlgorithm {
    #[serde(rename = "lossless-v1")]
    LosslessV1,
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) enum EventEncoding {
    #[serde(rename = "json-utf8-v1")]
    JsonUtf8V1,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct EventDescriptor {
    pub(crate) format: EventFormat,
    pub(crate) format_version: u16,
    pub(crate) algorithm: EventAlgorithm,
    pub(crate) encoding: EventEncoding,
    pub(crate) event_stream_id: String,
    pub(crate) state: CursorTrajectoryState,
    pub(crate) finalized: bool,
    pub(crate) total_blocks: u64,
    pub(crate) total_records: u64,
    pub(crate) total_bytes: u64,
    pub(crate) stream_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct EventReceipt {
    pub(crate) version: u16,
    pub(crate) durable: bool,
    pub(crate) format: EventFormat,
    pub(crate) state: CursorTrajectoryState,
    pub(crate) total_blocks: u64,
    pub(crate) total_records: u64,
    pub(crate) total_bytes: u64,
    pub(crate) stream_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct EventManifestEntry {
    pub(crate) sequence: u64,
    pub(crate) start_time_us: u64,
    pub(crate) end_time_us: u64,
    pub(crate) record_count: u16,
    pub(crate) byte_length: u32,
    pub(crate) block_sha256: String,
    pub(crate) prefix_sha256: String,
}

impl EventDescriptor {
    pub(crate) fn validate(&self) -> Result<(), EventContractError> {
        let counts = [self.total_blocks, self.total_records, self.total_bytes];
        if self.format_version != 1
            || !valid_trajectory_id(&self.event_stream_id)
            || !valid_trajectory_hash(&self.stream_sha256)
            || counts.into_iter().any(|count| count > MAX_SAFE_INTEGER)
            || self.total_blocks > self.total_records
            || u128::from(self.total_records) > u128::from(self.total_blocks) * 512
            || u128::from(self.total_bytes) > u128::from(self.total_blocks) * 65536
            || self.total_bytes < self.total_blocks
            || (self.total_blocks == 0
                && (self.total_records != 0
                    || self.total_bytes != 0
                    || self.stream_sha256
                        != "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"))
            || (self.state == CursorTrajectoryState::Unavailable && !self.finalized)
        {
            return Err(EventContractError::Invalid);
        }
        Ok(())
    }
}

impl EventReceipt {
    #[cfg(any(windows, test))]
    pub(crate) fn matches(&self, descriptor: &EventDescriptor) -> bool {
        descriptor.finalized
            && self.version == 1
            && self.durable
            && self.format == descriptor.format
            && self.state == descriptor.state
            && self.total_blocks == descriptor.total_blocks
            && self.total_records == descriptor.total_records
            && self.total_bytes == descriptor.total_bytes
            && self.stream_sha256 == descriptor.stream_sha256
    }
}
