//! Safe tray exit policy while recording work is active.

use crate::tray::{TrayCommandSource, TrayNoticeKind, TrayRecordingStatus};

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
