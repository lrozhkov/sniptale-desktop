use super::super::transfer_record::ScreenshotTransferRecordError;
#[cfg(any(windows, test))]
use crate::transfer::{chunks::EncodedChunk, error::TransferError};

const DEFAULT_INCOMPLETE_TTL_MS: u64 = 60 * 60 * 1_000;
const DEFAULT_FINALIZED_TTL_MS: u64 = 24 * 60 * 60 * 1_000;
const DEFAULT_REJECTED_TTL_MS: u64 = 60 * 60 * 1_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ScreenshotTransferStoreConfig {
    pub(crate) writing_expiry: u64,
    pub(crate) finalized_expiry: u64,
    pub(crate) rejected_retention: u64,
}

impl Default for ScreenshotTransferStoreConfig {
    fn default() -> Self {
        Self {
            writing_expiry: DEFAULT_INCOMPLETE_TTL_MS,
            finalized_expiry: DEFAULT_FINALIZED_TTL_MS,
            rejected_retention: DEFAULT_REJECTED_TTL_MS,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ScreenshotStartupRecoverySummary {
    pub(crate) pending_screenshots: usize,
    pub(crate) keep_writing: usize,
    pub(crate) retained_rejected: usize,
    pub(crate) deleted: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ScreenshotTransferStoreError {
    Io,
    InvalidMetadata,
    InvalidRecord(ScreenshotTransferRecordError),
    #[cfg(any(windows, test))]
    Protection,
    #[cfg(any(windows, test))]
    Transfer(TransferError),
}

impl From<ScreenshotTransferRecordError> for ScreenshotTransferStoreError {
    fn from(error: ScreenshotTransferRecordError) -> Self {
        Self::InvalidRecord(error)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(any(windows, test))]
pub(crate) struct ScreenshotTransferChunk {
    pub(crate) chunk: EncodedChunk,
    pub(crate) is_complete: bool,
}
