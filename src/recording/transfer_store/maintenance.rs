//! Recording transfer cache startup and periodic cleanup.

use super::{RecordingTransferStore, RecordingTransferStoreError, StartupRecoverySummary};
use crate::recording::temp_media::StartupRecoveryDecision;
use crate::transfer::lifecycle::cleanup_recovery_decision;

impl RecordingTransferStore {
    #[cfg(not(test))]
    pub(crate) fn startup_recovery_summary(
        &self,
    ) -> Result<StartupRecoverySummary, RecordingTransferStoreError> {
        self.startup_recovery_summary_at(super::system_epoch_ms())
    }

    pub(crate) fn startup_recovery_summary_at(
        &self,
        now_ms: u64,
    ) -> Result<StartupRecoverySummary, RecordingTransferStoreError> {
        self.cleanup_records_at(now_ms, true)
    }

    pub(crate) fn cleanup_expired(
        &self,
    ) -> Result<StartupRecoverySummary, RecordingTransferStoreError> {
        self.cleanup_expired_at(super::system_epoch_ms())
    }

    pub(crate) fn cleanup_expired_at(
        &self,
        now_ms: u64,
    ) -> Result<StartupRecoverySummary, RecordingTransferStoreError> {
        self.cleanup_records_at(now_ms, false)
    }

    fn cleanup_records_at(
        &self,
        now_ms: u64,
        delete_writing: bool,
    ) -> Result<StartupRecoverySummary, RecordingTransferStoreError> {
        let recovered = self
            .disk
            .scan_recovery(
                now_ms,
                self.config.writing_expiry,
                self.config.finalized_expiry,
            )
            .map_err(RecordingTransferStoreError::Disk)?;
        let mut summary = StartupRecoverySummary::default();
        for item in recovered {
            match cleanup_recovery_decision(item.decision, item.record.is_writing(), delete_writing)
            {
                StartupRecoveryDecision::KeepWriting => summary.keep_writing += 1,
                StartupRecoveryDecision::ReportPending => summary.pending_recordings += 1,
                StartupRecoveryDecision::RetainRejected => summary.retained_rejected += 1,
                StartupRecoveryDecision::Delete => {
                    summary.deleted += 1;
                    self.delete_recording(&item.record.media_id)?;
                }
            }
        }
        Ok(summary)
    }
}
