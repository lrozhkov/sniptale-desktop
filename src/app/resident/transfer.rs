//! Resident transfer cache cleanup worker.

mod logging;

use std::{
    sync::mpsc::{Receiver, RecvTimeoutError, SyncSender},
    time::Duration,
};

use self::logging::{
    log_recording_cleanup_summary, log_screenshot_cleanup_summary, log_transfer_cleanup_warning,
    transfer_log,
};
use super::{AgentComponent, WorkerCommand, WorkerEvent};

const TRANSFER_CLEANUP_INTERVAL: Duration = Duration::from_mins(15);

pub(super) fn run_transfer_worker(
    component: AgentComponent,
    event_sender: &SyncSender<WorkerEvent>,
    command_receiver: &Receiver<WorkerCommand>,
    log: Option<&super::AgentProcessLogSink>,
) {
    let mut cleaner = DefaultTransferCacheCleaner::open(log);
    run_transfer_worker_with_cleaner(
        component,
        event_sender,
        command_receiver,
        &mut cleaner,
        TRANSFER_CLEANUP_INTERVAL,
    );
}

trait TransferCacheCleaner {
    fn cleanup_once(&mut self);
}

#[derive(Debug)]
struct DefaultTransferCacheCleaner {
    screenshots: Option<crate::capture::ScreenshotTransferStore>,
    recordings: Option<crate::recording::RecordingTransferStore>,
    #[cfg(all(windows, not(test)))]
    log: Option<super::AgentProcessLogSink>,
}

impl DefaultTransferCacheCleaner {
    fn open(log: Option<&super::AgentProcessLogSink>) -> Self {
        #[cfg(any(not(windows), test))]
        let screenshots = open_screenshot_store(log);
        #[cfg(any(not(windows), test))]
        let recordings = open_recording_store(log);
        #[cfg(all(windows, not(test)))]
        let log = log.cloned();
        #[cfg(all(windows, not(test)))]
        let screenshots = open_screenshot_store(log.as_ref());
        #[cfg(all(windows, not(test)))]
        let recordings = open_recording_store(log.as_ref());
        Self {
            screenshots,
            recordings,
            #[cfg(all(windows, not(test)))]
            log,
        }
    }
}

impl TransferCacheCleaner for DefaultTransferCacheCleaner {
    fn cleanup_once(&mut self) {
        if let Some(store) = &self.screenshots {
            match store.cleanup_expired() {
                Ok(summary) => log_screenshot_cleanup_summary(
                    transfer_log(self),
                    summary.pending_screenshots,
                    summary.keep_writing,
                    summary.retained_rejected,
                    summary.deleted,
                ),
                Err(_) => log_transfer_cleanup_warning(
                    transfer_log(self),
                    "screenshot-cleanup",
                    "screenshot transfer cleanup failed",
                    "transfer.screenshot_cleanup_failed",
                ),
            }
        }
        if let Some(store) = &self.recordings {
            match store.cleanup_expired() {
                Ok(summary) => log_recording_cleanup_summary(
                    transfer_log(self),
                    summary.pending_recordings,
                    summary.keep_writing,
                    summary.retained_rejected,
                    summary.deleted,
                ),
                Err(_) => log_transfer_cleanup_warning(
                    transfer_log(self),
                    "recording-cleanup",
                    "recording transfer cleanup failed",
                    "transfer.recording_cleanup_failed",
                ),
            }
        }
    }
}

fn open_screenshot_store(
    log: Option<&super::AgentProcessLogSink>,
) -> Option<crate::capture::ScreenshotTransferStore> {
    if let Ok(store) = crate::capture::ScreenshotTransferStore::open_default() {
        return Some(store);
    }
    log_transfer_cleanup_warning(
        log,
        "screenshot-store-open",
        "screenshot transfer store open failed",
        "transfer.screenshot_store_open_failed",
    );
    None
}

fn open_recording_store(
    log: Option<&super::AgentProcessLogSink>,
) -> Option<crate::recording::RecordingTransferStore> {
    if let Ok(store) = crate::recording::RecordingTransferStore::open_default() {
        return Some(store);
    }
    log_transfer_cleanup_warning(
        log,
        "recording-store-open",
        "recording transfer store open failed",
        "transfer.recording_store_open_failed",
    );
    None
}

fn run_transfer_worker_with_cleaner(
    component: AgentComponent,
    event_sender: &SyncSender<WorkerEvent>,
    command_receiver: &Receiver<WorkerCommand>,
    cleaner: &mut impl TransferCacheCleaner,
    interval: Duration,
) {
    let _ = event_sender.send(WorkerEvent::Started(component));
    cleaner.cleanup_once();
    loop {
        match command_receiver.recv_timeout(interval) {
            Ok(WorkerCommand::FenceMode { result_sender }) => {
                let _ = result_sender.send(false);
            }
            Ok(WorkerCommand::Stop) | Err(RecvTimeoutError::Disconnected) => break,
            Ok(
                WorkerCommand::ApplyHotkeyPlan { .. }
                | WorkerCommand::ManualHotkeyAction { .. }
                | WorkerCommand::RestoreHotkeys { .. }
                | WorkerCommand::ApplyTrayRegistry { .. }
                | WorkerCommand::ShowTrayNotice { .. }
                | WorkerCommand::UpdateTrayRecordingStatus { .. }
                | WorkerCommand::RefreshTrayConnectionStatus
                | WorkerCommand::CaptureScreenshot { .. }
                | WorkerCommand::StartRecording { .. }
                | WorkerCommand::ControlRecording { .. }
                | WorkerCommand::ControlActiveRecording { .. }
                | WorkerCommand::StartRecordingTelemetry { .. }
                | WorkerCommand::PauseRecordingTelemetry { .. }
                | WorkerCommand::ResumeRecordingTelemetry { .. }
                | WorkerCommand::FinishRecordingTelemetry { .. }
                | WorkerCommand::CancelRecordingTelemetry { .. }
                | WorkerCommand::DiscardRecordingTelemetry { .. }
                | WorkerCommand::SnapshotRecordingTelemetry { .. }
                | WorkerCommand::PrepareManualAsset { .. }
                | WorkerCommand::PublishManualAsset { .. },
            ) => {}
            #[cfg(test)]
            Ok(WorkerCommand::TrayMenuCommandSelected { .. }) => {}
            Err(RecvTimeoutError::Timeout) => cleaner.cleanup_once(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use super::*;

    #[test]
    fn transfer_worker_cleans_on_start_and_periodically_until_stop() {
        let (event_sender, event_receiver) = mpsc::sync_channel(1);
        let (command_sender, command_receiver) = mpsc::sync_channel(1);
        let mut cleaner = CountingCleaner {
            cleanups: 0,
            stop_after: 2,
            command_sender,
        };

        run_transfer_worker_with_cleaner(
            AgentComponent::TransferWorker,
            &event_sender,
            &command_receiver,
            &mut cleaner,
            Duration::from_millis(1),
        );

        assert_eq!(
            event_receiver.recv(),
            Ok(WorkerEvent::Started(AgentComponent::TransferWorker))
        );
        assert_eq!(cleaner.cleanups, 2);
    }

    #[derive(Debug)]
    struct CountingCleaner {
        cleanups: u32,
        stop_after: u32,
        command_sender: SyncSender<WorkerCommand>,
    }

    impl TransferCacheCleaner for CountingCleaner {
        fn cleanup_once(&mut self) {
            self.cleanups += 1;
            if self.cleanups == self.stop_after {
                let _ = self.command_sender.send(WorkerCommand::Stop);
            }
        }
    }
}
