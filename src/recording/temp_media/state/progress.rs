use super::{StartupRecoveryDecision, TempMediaRecord, TempMediaState, TempStoreError};
use crate::transfer::{
    error::TransferError,
    lifecycle::{TransferLifecycleError, TransferLifecyclePolicy},
};

impl TempMediaRecord {
    pub(crate) fn record_chunk_read(
        &mut self,
        chunk_index: u64,
        now_ms: u64,
    ) -> Result<(), TempStoreError> {
        let progress = self
            .lifecycle_snapshot()
            .record_chunk_read(chunk_index, now_ms)
            .map_err(|error| map_lifecycle_store_error(&error))?;
        self.next_missing_chunk_index = progress.next_missing_chunk_index;
        self.updated_at_ms = now_ms;
        Ok(())
    }

    pub(crate) fn validate_chunk_request(&self, chunk_index: u64) -> Result<(), TransferError> {
        self.lifecycle_snapshot()
            .ensure_expected_chunk(chunk_index)
            .map_err(map_lifecycle_transfer_error)
    }

    pub(crate) fn ack(&mut self, now_ms: u64) -> Result<(), TempStoreError> {
        self.lifecycle_snapshot()
            .acknowledge_for_cleanup(now_ms)
            .map_err(|error| map_lifecycle_store_error(&error))?;
        self.state = TempMediaState::Acked;
        self.updated_at_ms = now_ms;
        Ok(())
    }

    pub(crate) fn reject(
        &mut self,
        now_ms: u64,
        retain_until_ms: Option<u64>,
    ) -> Result<(), TempStoreError> {
        self.lifecycle_snapshot()
            .reject_for_terminal(now_ms, retain_until_ms.is_some())
            .map_err(|error| map_lifecycle_store_error(&error))?;
        self.state = TempMediaState::Rejected;
        self.retain_rejected_until_ms = retain_until_ms;
        self.updated_at_ms = now_ms;
        Ok(())
    }

    pub(crate) fn recovery_decision(
        &self,
        now_ms: u64,
        incomplete_ttl_ms: u64,
        finalized_ttl_ms: u64,
    ) -> Result<StartupRecoveryDecision, TempStoreError> {
        self.lifecycle_snapshot()
            .recovery_decision(
                now_ms,
                TransferLifecyclePolicy {
                    writing_ttl_ms: incomplete_ttl_ms,
                    finalized_ttl_ms,
                },
            )
            .map_err(|error| map_lifecycle_store_error(&error))
    }
}

pub(super) fn map_lifecycle_store_error(error: &TransferLifecycleError) -> TempStoreError {
    if matches!(error, TransferLifecycleError::ClockMovedBackwards) {
        TempStoreError::ClockMovedBackwards
    } else {
        TempStoreError::InvalidTransition
    }
}

fn map_lifecycle_transfer_error(error: TransferLifecycleError) -> TransferError {
    match error {
        TransferLifecycleError::Transfer(error) => error,
        TransferLifecycleError::ClockMovedBackwards | TransferLifecycleError::InvalidState => {
            TransferError::InvalidLimit
        }
    }
}
