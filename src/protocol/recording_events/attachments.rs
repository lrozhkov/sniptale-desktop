//! Independently negotiated event attachment envelopes; receipt counts never include samples.

use super::{EventDescriptor, EventFormat, EventManifestEntry, EventReceipt};
use serde::{Deserialize, Serialize};

#[cfg(test)]
pub(crate) const RECORDING_EVENTS_CAPABILITY: &str = "recording-events-v1";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct NativeEventAttachmentCapabilities {
    pub(crate) recording_event_formats: Vec<EventFormat>,
    pub(crate) max_block_bytes: u32,
    pub(crate) max_block_records: u16,
    pub(crate) max_chunk_raw_bytes: u32,
    pub(crate) max_manifest_entries: u16,
}

impl NativeEventAttachmentCapabilities {
    pub(crate) fn v1() -> Self {
        Self {
            recording_event_formats: vec![EventFormat::V1],
            max_block_bytes: 65536,
            max_block_records: 512,
            max_chunk_raw_bytes: 32768,
            max_manifest_entries: 32,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ExtensionRecordingEventsManifestRequestMessage {
    pub(crate) protocol_version: u16,
    pub(crate) controller_lease_id: String,
    pub(crate) recording_id: String,
    pub(crate) event_stream_id: String,
    pub(crate) request_id: String,
    pub(crate) next_sequence: u64,
    pub(crate) prefix_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AppRecordingEventsManifestMessage {
    pub(crate) protocol_version: u16,
    pub(crate) controller_lease_id: String,
    pub(crate) recording_id: String,
    pub(crate) event_stream_id: String,
    pub(crate) request_id: String,
    pub(crate) descriptor: EventDescriptor,
    pub(crate) from_sequence: u64,
    pub(crate) entries: Vec<EventManifestEntry>,
    pub(crate) next_sequence: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ExtensionRecordingEventsChunkRequestMessage {
    pub(crate) protocol_version: u16,
    pub(crate) controller_lease_id: String,
    pub(crate) recording_id: String,
    pub(crate) event_stream_id: String,
    pub(crate) request_id: String,
    pub(crate) sequence: u64,
    pub(crate) chunk_index: u8,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AppRecordingEventsChunkMessage {
    pub(crate) protocol_version: u16,
    pub(crate) controller_lease_id: String,
    pub(crate) recording_id: String,
    pub(crate) event_stream_id: String,
    pub(crate) request_id: String,
    pub(crate) sequence: u64,
    pub(crate) block_bytes: u32,
    pub(crate) block_sha256: String,
    pub(crate) chunk_index: u8,
    pub(crate) chunk_count: u8,
    pub(crate) chunk_byte_offset: u32,
    pub(crate) chunk_raw_bytes: u32,
    pub(crate) chunk_sha256: String,
    pub(crate) base64: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ExtensionRecordingEventsAckMessage {
    pub(crate) protocol_version: u16,
    pub(crate) controller_lease_id: String,
    pub(crate) recording_id: String,
    pub(crate) event_stream_id: String,
    pub(crate) request_id: String,
    pub(crate) receipt: EventReceipt,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AppRecordingEventsAvailableMessage {
    pub(crate) protocol_version: u16,
    pub(crate) controller_lease_id: String,
    pub(crate) recording_id: String,
    pub(crate) event_stream_id: String,
    pub(crate) video_sha256: String,
    pub(crate) descriptor: EventDescriptor,
}

#[cfg(test)]
#[test]
fn format_token_matches_serialized_format() -> Result<(), serde_json::Error> {
    assert_eq!(
        serde_json::to_value(super::EventFormat::V1)?,
        RECORDING_EVENTS_CAPABILITY
    );
    Ok(())
}
