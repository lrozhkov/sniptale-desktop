//! Durable browser storage receipt; assertions are checked against owner metadata.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct DurableMediaReceipt {
    pub(crate) durable: bool,
    pub(crate) sha256: String,
    pub(crate) total_bytes: u64,
}
