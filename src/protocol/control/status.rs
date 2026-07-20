use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::protocol::{media::failure::NativeAppError, platform::NativeInstallState};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExtensionPingMessage {
    pub(crate) protocol_version: u16,
    pub(crate) nonce: String,
    pub(crate) sent_at_epoch_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeAppStatus {
    pub(crate) connected_browser: bool,
    pub(crate) recording: Option<Value>,
    pub(crate) last_error: Option<NativeAppError>,
    pub(crate) settings_revision: Option<String>,
    pub(crate) install: NativeInstallState,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AppPongMessage {
    pub(crate) protocol_version: u16,
    pub(crate) nonce: String,
    pub(crate) sent_at_epoch_ms: u64,
    pub(crate) app_status: NativeAppStatus,
}
