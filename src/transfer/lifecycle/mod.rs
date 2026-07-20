//! Transfer-owned lifecycle policy for finalized media delivery.

use super::{chunks::chunk_count, error::TransferError, pending::PendingTransfer};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TransferLifecycleState {
    Writing,
    Finalized,
    Acked,
    Rejected,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TransferRecoveryDecision {
    KeepWriting,
    ReportPending,
    RetainRejected,
    Delete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TransferLifecyclePolicy {
    pub(crate) writing_ttl_ms: u64,
    pub(crate) finalized_ttl_ms: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TransferLifecycleError {
    ClockMovedBackwards,
    InvalidState,
    Transfer(TransferError),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TransferProgress {
    pub(crate) next_missing_chunk_index: u64,
    pub(crate) is_complete: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TransferLifecycleSnapshot<'a> {
    pub(crate) transfer_id: &'a str,
    pub(crate) state: TransferLifecycleState,
    pub(crate) updated_at_ms: u64,
    pub(crate) total_bytes: Option<u64>,
    pub(crate) next_missing_chunk_index: u64,
    pub(crate) retain_rejected_until_ms: Option<u64>,
}

pub(crate) const fn cleanup_recovery_decision(
    decision: TransferRecoveryDecision,
    is_writing: bool,
    delete_writing: bool,
) -> TransferRecoveryDecision {
    if delete_writing && is_writing {
        TransferRecoveryDecision::Delete
    } else {
        decision
    }
}

impl TransferLifecycleSnapshot<'_> {
    pub(crate) fn recovery_decision(
        &self,
        now_ms: u64,
        policy: TransferLifecyclePolicy,
    ) -> Result<TransferRecoveryDecision, TransferLifecycleError> {
        if now_ms < self.updated_at_ms {
            return Err(TransferLifecycleError::ClockMovedBackwards);
        }
        Ok(match self.state {
            TransferLifecycleState::Writing => expire_decision(
                now_ms,
                self.updated_at_ms,
                policy.writing_ttl_ms,
                TransferRecoveryDecision::KeepWriting,
            ),
            TransferLifecycleState::Finalized => expire_decision(
                now_ms,
                self.updated_at_ms,
                policy.finalized_ttl_ms,
                TransferRecoveryDecision::ReportPending,
            ),
            TransferLifecycleState::Acked => TransferRecoveryDecision::Delete,
            TransferLifecycleState::Rejected => match self.retain_rejected_until_ms {
                Some(retain_until_ms) if now_ms < retain_until_ms => {
                    TransferRecoveryDecision::RetainRejected
                }
                _ => TransferRecoveryDecision::Delete,
            },
        })
    }

    pub(crate) fn ensure_expected_chunk(
        &self,
        chunk_index: u64,
    ) -> Result<(), TransferLifecycleError> {
        self.finalized_pending_transfer()?
            .ensure_expected_chunk(chunk_index)
            .map_err(TransferLifecycleError::Transfer)
    }

    pub(crate) fn record_chunk_read(
        &self,
        chunk_index: u64,
        now_ms: u64,
    ) -> Result<TransferProgress, TransferLifecycleError> {
        self.ensure_time(now_ms)?;
        let mut pending = self.finalized_pending_transfer()?;
        pending
            .record_next_chunk(chunk_index)
            .map_err(TransferLifecycleError::Transfer)?;
        Ok(TransferProgress {
            next_missing_chunk_index: pending.next_missing_chunk_index(),
            is_complete: pending.is_complete(),
        })
    }

    pub(crate) fn acknowledge_for_cleanup(
        &self,
        now_ms: u64,
    ) -> Result<(), TransferLifecycleError> {
        self.ensure_time(now_ms)?;
        let mut pending = self.finalized_pending_transfer()?;
        pending
            .acknowledge_for_cleanup()
            .map_err(TransferLifecycleError::Transfer)
    }

    pub(crate) fn reject_for_terminal(
        &self,
        now_ms: u64,
        retain_for_diagnostics: bool,
    ) -> Result<(), TransferLifecycleError> {
        self.ensure_time(now_ms)?;
        let mut pending = self.finalized_pending_transfer()?;
        pending
            .reject(retain_for_diagnostics)
            .map_err(TransferLifecycleError::Transfer)
    }

    pub(crate) fn validate_finalized_progress(&self) -> Result<(), TransferLifecycleError> {
        if matches!(self.state, TransferLifecycleState::Writing) {
            return Err(TransferLifecycleError::InvalidState);
        }
        self.pending_transfer()?;
        Ok(())
    }

    pub(crate) fn validate_rejected_retention_window(&self) -> Result<(), TransferLifecycleError> {
        if let Some(retain_until_ms) = self.retain_rejected_until_ms
            && retain_until_ms < self.updated_at_ms
        {
            return Err(TransferLifecycleError::ClockMovedBackwards);
        }
        Ok(())
    }

    fn ensure_time(&self, now_ms: u64) -> Result<(), TransferLifecycleError> {
        if now_ms < self.updated_at_ms {
            Err(TransferLifecycleError::ClockMovedBackwards)
        } else {
            Ok(())
        }
    }

    fn finalized_pending_transfer(&self) -> Result<PendingTransfer, TransferLifecycleError> {
        if self.state != TransferLifecycleState::Finalized {
            return Err(TransferLifecycleError::InvalidState);
        }
        self.pending_transfer()
    }

    fn pending_transfer(&self) -> Result<PendingTransfer, TransferLifecycleError> {
        let total_bytes = self
            .total_bytes
            .ok_or(TransferLifecycleError::InvalidState)?;
        let chunks = chunk_count(total_bytes).map_err(TransferLifecycleError::Transfer)?;
        PendingTransfer::resume(self.transfer_id, chunks, self.next_missing_chunk_index)
            .map_err(TransferLifecycleError::Transfer)
    }
}

fn expire_decision(
    now_ms: u64,
    updated_at_ms: u64,
    ttl_ms: u64,
    keep_decision: TransferRecoveryDecision,
) -> TransferRecoveryDecision {
    if now_ms.saturating_sub(updated_at_ms) > ttl_ms {
        TransferRecoveryDecision::Delete
    } else {
        keep_decision
    }
}
