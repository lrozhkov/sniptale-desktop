use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ControllerAcquireReason {
    InitialConnect,
    UserRequestedTakeover,
    StaleControllerRecovery,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExtensionControllerAcquireMessage {
    pub(crate) protocol_version: u16,
    pub(crate) connection_id: String,
    pub(crate) browser_family: BrowserFamily,
    pub(crate) profile_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) document_id: Option<String>,
    pub(crate) requested_at_epoch_ms: u64,
    pub(crate) reason: ControllerAcquireReason,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum BrowserFamily {
    Chrome,
    Edge,
    Chromium,
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeControllerIdentity {
    pub(crate) extension_id: String,
    pub(crate) browser_family: BrowserFamily,
    pub(crate) profile_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) document_id: Option<String>,
    pub(crate) connection_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ControllerLeaseStatus {
    Granted,
    OwnedByOtherProfile,
    Rejected,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AppControllerLeaseMessage {
    pub(crate) protocol_version: u16,
    pub(crate) controller_lease_id: String,
    pub(crate) controller: NativeControllerIdentity,
    pub(crate) expires_at_epoch_ms: u64,
    pub(crate) status: ControllerLeaseStatus,
}
