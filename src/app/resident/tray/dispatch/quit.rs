//! Safe tray exit policy while recording work is active.

use crate::tray::{TrayCommandSource, TrayNoticeKind, TrayRecordingStatus};

const QUIT_DRAIN_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(10);
const QUIT_DRAIN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(1);

pub(in crate::app::resident::tray) fn prepare_user_quit(
    events: &crate::app::resident::AgentEventHub,
    source: &mut impl TrayCommandSource,
) -> bool {
    #[cfg(all(windows, not(test)))]
    if crate::settings::browser_startup_preference_path()
        .is_none_or(|path| crate::settings::persist_browser_start_allowed(&path, false).is_err())
    {
        let _ = source.show_notice(TrayNoticeKind::ActionRejected);
        return false;
    }
    #[cfg(any(not(windows), test))]
    let _ = source;
    let (receipt, expected) = events.publish_connection_closed();
    let started = std::time::Instant::now();
    while !receipt.completed(expected) && started.elapsed() < QUIT_DRAIN_TIMEOUT {
        std::thread::sleep(QUIT_DRAIN_POLL_INTERVAL);
    }
    true
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::app::resident::tray) enum QuitDisposition {
    ShutdownNow,
    WaitForRecordingIdle,
    Continue,
}

pub(in crate::app::resident::tray) fn dispatch(
    recording: TrayRecordingStatus,
    senders: &super::super::ManualWorkerSenders,
    source: &mut impl TrayCommandSource,
) -> QuitDisposition {
    if matches!(recording, TrayRecordingStatus::Idle) {
        return QuitDisposition::ShutdownNow;
    }
    if matches!(recording, TrayRecordingStatus::Finalizing) {
        let _ = source.show_notice(TrayNoticeKind::ActionRejected);
        return QuitDisposition::Continue;
    }
    let confirmed = source.confirm_stop_recording_and_exit().unwrap_or(false);
    if !confirmed {
        return QuitDisposition::Continue;
    }
    if senders
        .control_active_recording(crate::protocol::RecordingControl::Stop)
        .is_err()
    {
        let _ = source.show_notice(TrayNoticeKind::ActionRejected);
        return QuitDisposition::Continue;
    }
    QuitDisposition::WaitForRecordingIdle
}
