//! Recording transfer and progress protocol DTOs.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::failure::NativeRejectReason;
use crate::protocol::recording::{
    NativeEffectiveQualitySettings, NativeRecordingSource, NativeRequestedQualitySettings,
};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeRecordingTimebase {
    pub(crate) id: String,
    pub(crate) started_at_epoch_ms: u64,
    pub(crate) started_at_monotonic_ns: String,
    pub(crate) units: NativeRecordingTimebaseUnits,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum NativeRecordingTimebaseUnits {
    Milliseconds,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AppRecordingStartedMessage {
    pub(crate) protocol_version: u16,
    pub(crate) controller_lease_id: String,
    pub(crate) recording_id: String,
    pub(crate) source: NativeRecordingSource,
    pub(crate) timebase: NativeRecordingTimebase,
    pub(crate) requested_settings_revision: String,
    pub(crate) requested_quality: NativeRequestedQualitySettings,
    pub(crate) effective_quality: NativeEffectiveQualitySettings,
    pub(crate) started_at_epoch_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum RecordingProgressStatus {
    Recording,
    Paused,
    Stopping,
    Finalizing,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AppRecordingProgressMessage {
    pub(crate) protocol_version: u16,
    pub(crate) controller_lease_id: String,
    pub(crate) recording_id: String,
    pub(crate) duration_ms: u64,
    pub(crate) bytes_written: u64,
    pub(crate) status: RecordingProgressStatus,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeRecordingTelemetrySnapshot {
    pub(crate) viewport: Option<Value>,
    pub(crate) cursor_track: Option<Value>,
    pub(crate) action_events: Vec<Value>,
    pub(crate) signals: Vec<Value>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum RecordingMimeType {
    #[serde(rename = "video/mp4")]
    VideoMp4,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AppRecordingStoppedMessage {
    pub(crate) protocol_version: u16,
    pub(crate) controller_lease_id: String,
    pub(crate) recording_id: String,
    pub(crate) duration_ms: u64,
    pub(crate) filename: String,
    pub(crate) mime_type: RecordingMimeType,
    pub(crate) total_bytes: u64,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) fps: u16,
    pub(crate) chunk_count: u32,
    pub(crate) sha256: String,
    pub(crate) telemetry: Option<NativeRecordingTelemetrySnapshot>,
    pub(crate) open_editor: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExtensionRecordingChunkRequestMessage {
    pub(crate) protocol_version: u16,
    pub(crate) controller_lease_id: String,
    pub(crate) recording_id: String,
    pub(crate) chunk_index: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AppRecordingChunkMessage {
    pub(crate) protocol_version: u16,
    pub(crate) controller_lease_id: String,
    pub(crate) recording_id: String,
    pub(crate) chunk_index: u32,
    pub(crate) chunk_byte_offset: u64,
    pub(crate) chunk_raw_bytes: u64,
    pub(crate) chunk_sha256: String,
    pub(crate) base64: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExtensionRecordingAckMessage {
    pub(crate) protocol_version: u16,
    pub(crate) controller_lease_id: String,
    pub(crate) recording_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExtensionRecordingRejectMessage {
    pub(crate) protocol_version: u16,
    pub(crate) controller_lease_id: String,
    pub(crate) recording_id: String,
    pub(crate) reason: NativeRejectReason,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) diagnostic_retention_requested: Option<bool>,
}
