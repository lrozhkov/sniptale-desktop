//! Event-stream commit head with independent video and event receipts.

use super::{
    protected_key::ProtectedMediaKey,
    trajectory_crypto::{HistoryProtection, TrajectoryCipher},
    trajectory_ledger::{TrajectoryStoreError, TrajectoryVideoBinding},
};
use crate::protocol::{
    DeliveryRoute,
    recording_events::EventDescriptor,
    trajectory::{MAX_SAFE_INTEGER, valid_trajectory_hash, valid_trajectory_id},
};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct EventsLedger {
    pub(super) version: u16,
    pub(super) recording_id: String,
    pub(super) event_stream_id: String,
    pub(super) writer_generation: String,
    pub(super) key: ProtectedMediaKey,
    pub(super) route: DeliveryRoute,
    pub(super) video: Option<TrajectoryVideoBinding>,
    pub(super) video_receipt_accepted: bool,
    pub(super) event_receipt_accepted: bool,
    pub(super) retired: bool,
    pub(super) committed_data_bytes: u64,
    pub(super) descriptor: EventDescriptor,
}

impl EventsLedger {
    pub(super) fn read(path: &Path) -> Result<(Self, TrajectoryCipher), TrajectoryStoreError> {
        super::history_head::read(path, |ledger: &Self| {
            ledger.validate()?;
            TrajectoryCipher::open_for(
                &ledger.recording_id,
                &ledger.event_stream_id,
                &ledger.key,
                HistoryProtection::Events,
            )
            .map_err(|_| TrajectoryStoreError::Authentication)
        })
    }

    pub(super) fn write(
        &self,
        path: &Path,
        cipher: &TrajectoryCipher,
    ) -> Result<(), TrajectoryStoreError> {
        self.validate()?;
        super::history_head::write(self, path, cipher)
    }

    fn validate(&self) -> Result<(), TrajectoryStoreError> {
        self.descriptor
            .validate()
            .map_err(|_| TrajectoryStoreError::Invalid)?;
        if self.version != 1
            || self.descriptor.event_stream_id != self.event_stream_id
            || !valid_trajectory_id(&self.writer_generation)
            || !self.route.valid_local_route()
            || self.recording_id.is_empty()
            || self.recording_id.len() > 128
            || !self.recording_id.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-')
            })
            || self.video.as_ref().is_some_and(|video| {
                !valid_trajectory_hash(&video.sha256)
                    || video.bytes == 0
                    || video.bytes > MAX_SAFE_INTEGER
                    || video.duration_ms > MAX_SAFE_INTEGER
                    || video.width == 0
                    || video.height == 0
            })
            || (self.video_receipt_accepted && self.video.is_none())
            || (self.event_receipt_accepted && !self.descriptor.finalized)
        {
            return Err(TrajectoryStoreError::Invalid);
        }
        Ok(())
    }
}
