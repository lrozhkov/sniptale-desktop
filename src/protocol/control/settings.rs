use serde::{Deserialize, Serialize};
use serde_json::Number;

use crate::protocol::recording::{
    NativeAudioSourceMode, NativeSettingsWarning, NativeVideoFrameRate,
};

use super::tray::NativeTrayActionRegistry;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeScreenshotSettings {
    pub(crate) include_cursor: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum NativeHardwareAcceleration {
    Prefer,
    ForceSoftware,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum NativeContainerSetting {
    Mp4,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum NativeVideoCodecSetting {
    H264,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum NativeAudioCodecSetting {
    Aac,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeVideoAdvancedSettings {
    pub(crate) audio_bitrate_kbps: u16,
    pub(crate) audio_source_mode: NativeAudioSourceMode,
    pub(crate) frame_rate: NativeVideoFrameRate,
    pub(crate) include_cursor_in_video: bool,
    pub(crate) max_duration_minutes: Number,
    pub(crate) prefer_hardware_encoder: bool,
    pub(crate) video_bitrate_mbps_override: Option<Number>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeVideoCodecSettings {
    pub(crate) audio_codec: NativeAudioCodecSetting,
    pub(crate) container: NativeContainerSetting,
    pub(crate) hardware_acceleration: NativeHardwareAcceleration,
    pub(crate) video_codec: NativeVideoCodecSetting,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[expect(
    clippy::struct_excessive_bools,
    reason = "wire DTO exposes independent telemetry feature flags from settings schema"
)]
pub(crate) struct NativeVideoTelemetrySettings {
    pub(crate) collect_cursor: bool,
    pub(crate) collect_clicks: bool,
    pub(crate) collect_key_events: bool,
    pub(crate) collect_typing_spans: bool,
    pub(crate) collect_static_signals: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeVideoSettings {
    pub(crate) enabled: bool,
    pub(crate) advanced: NativeVideoAdvancedSettings,
    pub(crate) codec: NativeVideoCodecSettings,
    pub(crate) telemetry: NativeVideoTelemetrySettings,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeSettingsSnapshot {
    pub(crate) screenshots: NativeScreenshotSettings,
    pub(crate) video: NativeVideoSettings,
    pub(crate) tray_actions: NativeTrayActionRegistry,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeEffectiveSettings {
    pub(crate) screenshots: NativeScreenshotSettings,
    pub(crate) video: NativeVideoSettings,
    pub(crate) warnings: Vec<NativeSettingsWarning>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExtensionSettingsSyncMessage {
    pub(crate) protocol_version: u16,
    pub(crate) controller_lease_id: String,
    pub(crate) revision: String,
    pub(crate) schema_version: u16,
    pub(crate) generated_at_epoch_ms: u64,
    pub(crate) settings: NativeSettingsSnapshot,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AppSettingsAcceptedMessage {
    pub(crate) protocol_version: u16,
    pub(crate) controller_lease_id: String,
    pub(crate) revision: String,
    pub(crate) schema_version: u16,
    pub(crate) accepted_at_epoch_ms: u64,
    pub(crate) effective_settings: NativeEffectiveSettings,
    pub(crate) warnings: Vec<NativeSettingsWarning>,
}
