//! Screenshot transfer cache owner split by store, service, maintenance, protection, and reconnect roles.

mod maintenance;
#[cfg(any(windows, test))]
mod manual;
#[cfg(any(windows, test))]
mod protection;
#[cfg(any(windows, test))]
mod reconnect;
mod service;
mod types;

use super::transfer_disk::ScreenshotTransferDiskStore;

#[cfg(any(windows, test))]
pub(crate) use reconnect::PendingScreenshotTransfer;
#[cfg(any(windows, test))]
pub(crate) use types::ScreenshotTransferChunk;
pub(crate) use types::{
    ScreenshotStartupRecoverySummary, ScreenshotTransferStoreConfig, ScreenshotTransferStoreError,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ScreenshotTransferStore {
    pub(super) disk: ScreenshotTransferDiskStore,
    pub(super) config: ScreenshotTransferStoreConfig,
}
