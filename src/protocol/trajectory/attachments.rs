//! Negotiated bounded cursor attachment messages; video receipts remain separate.

use super::{
    CursorTrajectoryDescriptor, CursorTrajectoryFormat, CursorTrajectoryManifestEntry,
    CursorTrajectoryReceipt,
};
use serde::{Deserialize, Serialize};

pub(crate) const CURSOR_TRAJECTORY_CAPABILITY: &str = "cursor-trajectory-v1";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct NativeTrajectoryAttachmentCapabilities {
    pub(crate) cursor_trajectory_formats: Vec<CursorTrajectoryFormat>,
    pub(crate) max_block_bytes: u32,
    pub(crate) max_block_records: u16,
    pub(crate) max_chunk_raw_bytes: u32,
    pub(crate) max_manifest_entries: u16,
}

impl NativeTrajectoryAttachmentCapabilities {
    pub(crate) fn v1() -> Self {
        Self {
            cursor_trajectory_formats: vec![CursorTrajectoryFormat::V1],
            max_block_bytes: 65536,
            max_block_records: 512,
            max_chunk_raw_bytes: 32768,
            max_manifest_entries: 32,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ExtensionRecordingAttachmentManifestRequestMessage {
    pub(crate) protocol_version: u16,
    pub(crate) controller_lease_id: String,
    pub(crate) recording_id: String,
    pub(crate) trajectory_id: String,
    pub(crate) request_id: String,
    pub(crate) next_sequence: u64,
    pub(crate) prefix_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AppRecordingAttachmentManifestMessage {
    pub(crate) protocol_version: u16,
    pub(crate) controller_lease_id: String,
    pub(crate) recording_id: String,
    pub(crate) trajectory_id: String,
    pub(crate) request_id: String,
    pub(crate) descriptor: CursorTrajectoryDescriptor,
    pub(crate) from_sequence: u64,
    pub(crate) entries: Vec<CursorTrajectoryManifestEntry>,
    pub(crate) next_sequence: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ExtensionRecordingAttachmentChunkRequestMessage {
    pub(crate) protocol_version: u16,
    pub(crate) controller_lease_id: String,
    pub(crate) recording_id: String,
    pub(crate) trajectory_id: String,
    pub(crate) request_id: String,
    pub(crate) sequence: u64,
    pub(crate) chunk_index: u8,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AppRecordingAttachmentChunkMessage {
    pub(crate) protocol_version: u16,
    pub(crate) controller_lease_id: String,
    pub(crate) recording_id: String,
    pub(crate) trajectory_id: String,
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
pub(crate) struct ExtensionRecordingAttachmentAckMessage {
    pub(crate) protocol_version: u16,
    pub(crate) controller_lease_id: String,
    pub(crate) recording_id: String,
    pub(crate) trajectory_id: String,
    pub(crate) request_id: String,
    pub(crate) receipt: CursorTrajectoryReceipt,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AppRecordingAttachmentAvailableMessage {
    pub(crate) protocol_version: u16,
    pub(crate) controller_lease_id: String,
    pub(crate) recording_id: String,
    pub(crate) trajectory_id: String,
    pub(crate) video_sha256: String,
    pub(crate) descriptor: CursorTrajectoryDescriptor,
}

#[cfg(test)]
#[test]
fn format_token_matches_serialized_format() -> Result<(), serde_json::Error> {
    assert_eq!(
        serde_json::to_value(super::CursorTrajectoryFormat::V1)?,
        CURSOR_TRAJECTORY_CAPABILITY
    );
    Ok(())
}
