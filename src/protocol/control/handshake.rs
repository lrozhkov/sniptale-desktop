use serde::{Deserialize, Serialize};

use crate::protocol::{
    capabilities::NativeAppCapabilities,
    platform::{NativeInstallState, NativePlatform},
};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExtensionHelloMessage {
    pub(crate) protocol_version: u16,
    pub(crate) settings_schema_version: u16,
    pub(crate) extension_version: String,
    pub(crate) extension_id: String,
    pub(crate) min_app_version: String,
    pub(crate) supported_protocol_versions: Vec<u16>,
    pub(crate) supported_settings_schema_versions: Vec<u16>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AppHelloMessage {
    pub(crate) protocol_version: u16,
    pub(crate) settings_schema_version: u16,
    pub(crate) min_extension_version: String,
    pub(crate) supported_protocol_versions: Vec<u16>,
    pub(crate) supported_settings_schema_versions: Vec<u16>,
    pub(crate) app_instance_id: String,
    pub(crate) platform: NativePlatform,
    pub(crate) install: NativeInstallState,
    pub(crate) capabilities: NativeAppCapabilities,
}
