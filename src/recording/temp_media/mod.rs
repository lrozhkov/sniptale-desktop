//! Crash-safe temporary recording media metadata and disk state.

mod disk;
mod metadata;
mod state;

pub(crate) use disk::{RecoveredTempMedia, TempMediaDiskStore, TempStoreDiskError};
pub(crate) use metadata::RecordingMediaMetadata;
#[cfg(test)]
pub(crate) use state::TempMediaState;
pub(crate) use state::{StartupRecoveryDecision, TempMediaRecord, TempStoreError};
