//! Screenshot transfer metadata validation and recovery decisions.
#[cfg(any(windows, test))]
mod route;
mod validation;

use serde::{Deserialize, Serialize};

use crate::{
    protocol::{DeliveryRoute, OperationAuthority, ScreenshotMode},
    transfer::{
        chunks::chunk_count,
        error::TransferError,
        lifecycle::{
            TransferLifecycleError, TransferLifecyclePolicy, TransferLifecycleSnapshot,
            TransferLifecycleState,
        },
        protected_file::PROTECTED_MEDIA_ALGORITHM,
        protected_key::{ProtectedMediaKey, validate_protected_media_key},
    },
};

pub(super) use crate::transfer::lifecycle::TransferRecoveryDecision as RecoveryDecision;
#[cfg(any(windows, test))]
pub(crate) use route::{FinalizedScreenshotTransfer, PreparedScreenshotTransfer};
pub(super) use validation::validate_capture_id;
use validation::{validate_filename, validate_sha256};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(super) enum ScreenshotTransferState {
    Writing,
    Finalized,
    Rejected,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ScreenshotTransferRecord {
    pub(super) capture_id: String,
    pub(super) state: ScreenshotTransferState,
    pub(super) created_at_ms: u64,
    pub(super) updated_at_ms: u64,
    pub(super) mode: ScreenshotMode,
    pub(super) filename: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) route_schema: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) authority: Option<OperationAuthority>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) delivery_route: Option<DeliveryRoute>,
    pub(super) x: i32,
    pub(super) y: i32,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) total_bytes: Option<u64>,
    pub(super) sha256: Option<String>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub(super) next_missing_chunk_index: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) media_protection: Option<ProtectedMediaKey>,
    pub(super) open_editor: bool,
    pub(super) retain_rejected_until_ms: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ScreenshotTransferRecordError {
    InvalidId,
    InvalidFilename,
    InvalidState,
    Transfer(TransferError),
}

impl ScreenshotTransferRecord {
    pub(super) fn recovery_decision(
        &self,
        now_ms: u64,
        incomplete_ttl_ms: u64,
        finalized_ttl_ms: u64,
    ) -> Result<RecoveryDecision, ScreenshotTransferRecordError> {
        self.lifecycle_snapshot()
            .recovery_decision(
                now_ms,
                TransferLifecyclePolicy {
                    writing_ttl_ms: incomplete_ttl_ms,
                    finalized_ttl_ms,
                },
            )
            .map_err(Self::map_lifecycle_error)
    }

    pub(super) const fn is_writing(&self) -> bool {
        matches!(self.state, ScreenshotTransferState::Writing)
    }

    pub(super) fn record_chunk_read(
        &mut self,
        chunk_index: u64,
        now_ms: u64,
    ) -> Result<bool, ScreenshotTransferRecordError> {
        let progress = self
            .lifecycle_snapshot()
            .record_chunk_read(chunk_index, now_ms)
            .map_err(Self::map_lifecycle_error)?;
        self.next_missing_chunk_index = progress.next_missing_chunk_index;
        self.updated_at_ms = now_ms;
        Ok(progress.is_complete)
    }

    pub(super) fn validate_chunk_request(
        &self,
        chunk_index: u64,
    ) -> Result<(), ScreenshotTransferRecordError> {
        self.lifecycle_snapshot()
            .ensure_expected_chunk(chunk_index)
            .map_err(Self::map_lifecycle_error)
    }

    pub(super) fn ack(&mut self, now_ms: u64) -> Result<(), ScreenshotTransferRecordError> {
        self.lifecycle_snapshot()
            .acknowledge_for_cleanup(now_ms)
            .map_err(Self::map_lifecycle_error)?;
        self.updated_at_ms = now_ms;
        Ok(())
    }

    pub(super) fn reject(
        &mut self,
        now_ms: u64,
        retain_until_ms: Option<u64>,
    ) -> Result<(), ScreenshotTransferRecordError> {
        self.lifecycle_snapshot()
            .reject_for_terminal(now_ms, retain_until_ms.is_some())
            .map_err(Self::map_lifecycle_error)?;
        self.state = ScreenshotTransferState::Rejected;
        self.retain_rejected_until_ms = retain_until_ms;
        self.updated_at_ms = now_ms;
        Ok(())
    }

    pub(super) fn validate(&self) -> Result<(), ScreenshotTransferRecordError> {
        validate_capture_id(&self.capture_id)?;
        validate_filename(&self.filename)?;
        if self.updated_at_ms < self.created_at_ms || self.width == 0 || self.height == 0 {
            return Err(ScreenshotTransferRecordError::InvalidState);
        }
        self.validate_route_snapshot()?;
        if self.state == ScreenshotTransferState::Writing {
            return self.validate_writing_shape();
        }
        let total_bytes = self
            .total_bytes
            .ok_or(ScreenshotTransferRecordError::InvalidState)?;
        let chunks = chunk_count(total_bytes).map_err(ScreenshotTransferRecordError::Transfer)?;
        let _ = chunks;
        self.lifecycle_snapshot()
            .validate_finalized_progress()
            .map_err(Self::map_lifecycle_error)?;
        validate_sha256(
            self.sha256
                .as_deref()
                .ok_or(ScreenshotTransferRecordError::InvalidState)?,
        )?;
        self.lifecycle_snapshot()
            .validate_rejected_retention_window()
            .map_err(Self::map_lifecycle_error)?;
        if let Some(protection) = &self.media_protection {
            validate_protected_media_key(protection, PROTECTED_MEDIA_ALGORITHM)
                .map_err(|_| ScreenshotTransferRecordError::InvalidState)?;
        }
        Ok(())
    }

    fn validate_writing_shape(&self) -> Result<(), ScreenshotTransferRecordError> {
        if self.total_bytes.is_none()
            && self.sha256.is_none()
            && self.media_protection.is_none()
            && self.retain_rejected_until_ms.is_none()
            && self.next_missing_chunk_index == 0
        {
            Ok(())
        } else {
            Err(ScreenshotTransferRecordError::InvalidState)
        }
    }

    fn lifecycle_snapshot(&self) -> TransferLifecycleSnapshot<'_> {
        TransferLifecycleSnapshot {
            transfer_id: &self.capture_id,
            state: match self.state {
                ScreenshotTransferState::Writing => TransferLifecycleState::Writing,
                ScreenshotTransferState::Finalized => TransferLifecycleState::Finalized,
                ScreenshotTransferState::Rejected => TransferLifecycleState::Rejected,
            },
            updated_at_ms: self.updated_at_ms,
            total_bytes: self.total_bytes,
            next_missing_chunk_index: self.next_missing_chunk_index,
            retain_rejected_until_ms: self.retain_rejected_until_ms,
        }
    }

    fn validate_route_snapshot(&self) -> Result<(), ScreenshotTransferRecordError> {
        match (
            self.route_schema,
            self.authority.as_ref(),
            self.delivery_route.as_ref(),
        ) {
            (None, None, None)
            | (
                Some(1),
                Some(OperationAuthority::ExtensionManaged { .. }),
                Some(DeliveryRoute::ExtensionTransfer),
            ) => Ok(()),
            (
                Some(1),
                Some(OperationAuthority::Manual { .. }),
                Some(DeliveryRoute::LocalAsset { asset_job_id }),
            ) if !asset_job_id.is_empty() => Ok(()),
            _ => Err(ScreenshotTransferRecordError::InvalidState),
        }
    }

    fn map_lifecycle_error(error: TransferLifecycleError) -> ScreenshotTransferRecordError {
        match error {
            TransferLifecycleError::Transfer(error) => {
                ScreenshotTransferRecordError::Transfer(error)
            }
            TransferLifecycleError::ClockMovedBackwards | TransferLifecycleError::InvalidState => {
                ScreenshotTransferRecordError::InvalidState
            }
        }
    }
}

#[expect(clippy::trivially_copy_pass_by_ref, reason = "serde predicate")]
const fn is_zero(value: &u64) -> bool {
    *value == 0
}
