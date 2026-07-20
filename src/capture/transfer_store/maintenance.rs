//! Screenshot transfer startup and periodic cleanup.

use super::{
    super::transfer_record::RecoveryDecision, ScreenshotStartupRecoverySummary,
    ScreenshotTransferStore, ScreenshotTransferStoreError, service::system_epoch_ms,
};
use crate::transfer::lifecycle::cleanup_recovery_decision;

impl ScreenshotTransferStore {
    pub(crate) fn startup_recovery_summary(
        &self,
    ) -> Result<ScreenshotStartupRecoverySummary, ScreenshotTransferStoreError> {
        self.startup_recovery_summary_at(system_epoch_ms())
    }

    pub(crate) fn startup_recovery_summary_at(
        &self,
        now_ms: u64,
    ) -> Result<ScreenshotStartupRecoverySummary, ScreenshotTransferStoreError> {
        self.cleanup_records_at(now_ms, true)
    }

    #[cfg(any(windows, test))]
    pub(crate) fn cleanup_expired(
        &self,
    ) -> Result<ScreenshotStartupRecoverySummary, ScreenshotTransferStoreError> {
        self.cleanup_expired_at(system_epoch_ms())
    }

    #[cfg(any(windows, test))]
    pub(crate) fn cleanup_expired_at(
        &self,
        now_ms: u64,
    ) -> Result<ScreenshotStartupRecoverySummary, ScreenshotTransferStoreError> {
        self.cleanup_records_at(now_ms, false)
    }

    fn cleanup_records_at(
        &self,
        now_ms: u64,
        delete_writing: bool,
    ) -> Result<ScreenshotStartupRecoverySummary, ScreenshotTransferStoreError> {
        let mut summary = ScreenshotStartupRecoverySummary::default();
        for record in self.disk.scan_records()? {
            match cleanup_recovery_decision(
                self.recovery_decision(&record, now_ms)?,
                record.is_writing(),
                delete_writing,
            ) {
                RecoveryDecision::KeepWriting => summary.keep_writing += 1,
                RecoveryDecision::ReportPending => summary.pending_screenshots += 1,
                RecoveryDecision::RetainRejected => summary.retained_rejected += 1,
                RecoveryDecision::Delete => {
                    summary.deleted += 1;
                    self.disk.delete_capture(&record.capture_id)?;
                }
            }
        }
        Ok(summary)
    }
}
