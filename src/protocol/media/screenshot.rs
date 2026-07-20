//! Screenshot transfer protocol DTOs.

use serde::{Deserialize, Serialize};

use super::failure::NativeRejectReason;
use crate::protocol::capabilities::ScreenshotMode;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ScreenshotMimeType {
    #[serde(rename = "image/png")]
    ImagePng,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AppScreenshotStartMessage {
    pub(crate) protocol_version: u16,
    pub(crate) controller_lease_id: String,
    pub(crate) capture_id: String,
    pub(crate) mode: ScreenshotMode,
    pub(crate) filename: String,
    pub(crate) mime_type: ScreenshotMimeType,
    pub(crate) total_bytes: u64,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) chunk_count: u32,
    pub(crate) sha256: String,
    pub(crate) open_editor: bool,
    pub(crate) captured_at_epoch_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AppScreenshotChunkMessage {
    pub(crate) protocol_version: u16,
    pub(crate) controller_lease_id: String,
    pub(crate) capture_id: String,
    pub(crate) chunk_index: u32,
    pub(crate) chunk_byte_offset: u64,
    pub(crate) chunk_raw_bytes: u64,
    pub(crate) chunk_sha256: String,
    pub(crate) base64: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AppScreenshotCommitMessage {
    pub(crate) protocol_version: u16,
    pub(crate) controller_lease_id: String,
    pub(crate) capture_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExtensionScreenshotChunkRequestMessage {
    pub(crate) protocol_version: u16,
    pub(crate) controller_lease_id: String,
    pub(crate) capture_id: String,
    pub(crate) chunk_index: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExtensionScreenshotAckMessage {
    pub(crate) protocol_version: u16,
    pub(crate) controller_lease_id: String,
    pub(crate) capture_id: String,
    pub(crate) asset_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExtensionScreenshotRejectMessage {
    pub(crate) protocol_version: u16,
    pub(crate) controller_lease_id: String,
    pub(crate) capture_id: String,
    pub(crate) reason: NativeRejectReason,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) diagnostic_retention_requested: Option<bool>,
}
