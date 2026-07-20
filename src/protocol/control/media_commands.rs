use serde::{Deserialize, Serialize};

use crate::protocol::{
    capabilities::ScreenshotMode,
    recording::{NativeRecordingSource, NativeRequestedQualitySettings},
};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExtensionScreenshotCaptureCommandMessage {
    pub(crate) protocol_version: u16,
    pub(crate) controller_lease_id: String,
    pub(crate) command_id: String,
    pub(crate) mode: ScreenshotMode,
    pub(crate) settings_revision: String,
    pub(crate) open_editor: bool,
    pub(crate) requested_at_epoch_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExtensionRecordingStartCommandMessage {
    pub(crate) protocol_version: u16,
    pub(crate) controller_lease_id: String,
    pub(crate) command_id: String,
    pub(crate) source: NativeRecordingSource,
    pub(crate) settings_revision: String,
    pub(crate) requested_quality: NativeRequestedQualitySettings,
    pub(crate) open_editor: bool,
    pub(crate) requested_at_epoch_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum RecordingControl {
    Pause,
    Resume,
    Stop,
    Cancel,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExtensionRecordingControlCommandMessage {
    pub(crate) protocol_version: u16,
    pub(crate) controller_lease_id: String,
    pub(crate) command_id: String,
    pub(crate) recording_id: String,
    pub(crate) control: RecordingControl,
    pub(crate) requested_at_epoch_ms: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum AcceptedOperation {
    Screenshot,
    Recording,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AppCommandAcceptedMessage {
    pub(crate) protocol_version: u16,
    pub(crate) controller_lease_id: String,
    pub(crate) command_id: String,
    pub(crate) operation: AcceptedOperation,
    pub(crate) accepted_at_epoch_ms: u64,
}
