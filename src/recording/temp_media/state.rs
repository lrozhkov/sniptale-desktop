//! Temporary media files, TTL, and pending completed media owner.
mod persistence;
mod progress;
mod validation;

use serde::{Deserialize, Serialize};

pub(crate) use super::metadata::RecordingMediaMetadata;
use super::metadata::validate_recording_metadata;
use crate::protocol::{DeliveryRoute, OperationAuthority};
use crate::transfer::{
    lifecycle::{TransferLifecycleSnapshot, TransferLifecycleState},
    protected_file::PROTECTED_MEDIA_ALGORITHM,
    protected_key::{ProtectedMediaKey, validate_protected_media_key},
};

pub(crate) use crate::transfer::lifecycle::TransferRecoveryDecision as StartupRecoveryDecision;

use self::validation::{validate_media_id, validate_sha256, validate_size};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) enum TempMediaState {
    Writing,
    Finalized,
    Acked,
    Rejected,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TempStoreError {
    EmptyMediaId,
    InvalidMediaId,
    InvalidFilename,
    InvalidSize,
    InvalidSha256,
    InvalidDuration,
    InvalidDimensions,
    InvalidFrameRate,
    InvalidTransition,
    ClockMovedBackwards,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct TempMediaRecord {
    pub(crate) media_id: String,
    pub(crate) state: TempMediaState,
    pub(crate) created_at_ms: u64,
    pub(crate) updated_at_ms: u64,
    pub(crate) total_bytes: Option<u64>,
    pub(crate) sha256: Option<String>,
    pub(crate) retain_rejected_until_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub(crate) next_missing_chunk_index: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) media_protection: Option<ProtectedMediaKey>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) recording: Option<RecordingMediaMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) route_schema: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) authority: Option<OperationAuthority>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) delivery_route: Option<DeliveryRoute>,
}

impl TempMediaRecord {
    pub(crate) fn start_writing(
        media_id: &str,
        created_at_ms: u64,
    ) -> Result<Self, TempStoreError> {
        validate_media_id(media_id)?;
        Ok(Self {
            media_id: media_id.to_owned(),
            state: TempMediaState::Writing,
            created_at_ms,
            updated_at_ms: created_at_ms,
            total_bytes: None,
            sha256: None,
            retain_rejected_until_ms: None,
            next_missing_chunk_index: 0,
            media_protection: None,
            recording: None,
            route_schema: None,
            authority: None,
            delivery_route: None,
        })
    }

    pub(crate) fn start_writing_with_route(
        media_id: &str,
        authority: OperationAuthority,
        delivery_route: DeliveryRoute,
        created_at_ms: u64,
    ) -> Result<Self, TempStoreError> {
        let mut record = Self::start_writing(media_id, created_at_ms)?;
        record.route_schema = Some(1);
        record.authority = Some(authority);
        record.delivery_route = Some(delivery_route);
        record.validate_for_persistence()?;
        Ok(record)
    }

    pub(crate) fn validate_media_id(media_id: &str) -> Result<(), TempStoreError> {
        validate_media_id(media_id)
    }

    pub(crate) fn finalize(
        &mut self,
        total_bytes: u64,
        sha256: &str,
        now_ms: u64,
    ) -> Result<(), TempStoreError> {
        self.ensure_state(TempMediaState::Writing)?;
        self.ensure_time(now_ms)?;
        validate_size(total_bytes)?;
        validate_sha256(sha256)?;
        self.state = TempMediaState::Finalized;
        self.total_bytes = Some(total_bytes);
        self.sha256 = Some(sha256.to_owned());
        self.updated_at_ms = now_ms;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn finalize_recording(
        &mut self,
        total_bytes: u64,
        sha256: &str,
        metadata: RecordingMediaMetadata,
        now_ms: u64,
    ) -> Result<(), TempStoreError> {
        self.finalize_recording_with_protection(total_bytes, sha256, metadata, None, now_ms)
    }

    pub(crate) fn finalize_recording_with_protection(
        &mut self,
        total_bytes: u64,
        sha256: &str,
        metadata: RecordingMediaMetadata,
        media_protection: Option<ProtectedMediaKey>,
        now_ms: u64,
    ) -> Result<(), TempStoreError> {
        validate_recording_metadata(&metadata)?;
        self.finalize(total_bytes, sha256, now_ms)?;
        self.media_protection = media_protection;
        self.recording = Some(metadata);
        Ok(())
    }

    pub(crate) const fn is_writing(&self) -> bool {
        matches!(self.state, TempMediaState::Writing)
    }

    fn ensure_state(&self, expected: TempMediaState) -> Result<(), TempStoreError> {
        if self.state == expected {
            Ok(())
        } else {
            Err(TempStoreError::InvalidTransition)
        }
    }

    fn ensure_time(&self, now_ms: u64) -> Result<(), TempStoreError> {
        if now_ms < self.updated_at_ms {
            Err(TempStoreError::ClockMovedBackwards)
        } else {
            Ok(())
        }
    }

    pub(super) fn lifecycle_snapshot(&self) -> TransferLifecycleSnapshot<'_> {
        TransferLifecycleSnapshot {
            transfer_id: &self.media_id,
            state: match self.state {
                TempMediaState::Writing => TransferLifecycleState::Writing,
                TempMediaState::Finalized => TransferLifecycleState::Finalized,
                TempMediaState::Acked => TransferLifecycleState::Acked,
                TempMediaState::Rejected => TransferLifecycleState::Rejected,
            },
            updated_at_ms: self.updated_at_ms,
            total_bytes: self.total_bytes,
            next_missing_chunk_index: self.next_missing_chunk_index,
            retain_rejected_until_ms: self.retain_rejected_until_ms,
        }
    }
}

#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde predicate takes reference"
)]
const fn is_zero(value: &u64) -> bool {
    *value == 0
}
