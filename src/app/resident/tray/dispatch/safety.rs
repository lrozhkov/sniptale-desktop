//! Local recording safety controls that never enter the extension wire namespace.

use crate::{
    protocol::RecordingControl,
    tray::{ShellActionId, TrayCommandSource, TrayNoticeKind},
};

pub(super) fn control_for_action(action_id: &str) -> Option<RecordingControl> {
    match action_id {
        id if id == ShellActionId::RecordingPause.as_str() => Some(RecordingControl::Pause),
        id if id == ShellActionId::RecordingResume.as_str() => Some(RecordingControl::Resume),
        id if id == ShellActionId::RecordingStop.as_str() => Some(RecordingControl::Stop),
        id if id == ShellActionId::RecordingCancel.as_str() => Some(RecordingControl::Cancel),
        _ => None,
    }
}

pub(super) fn dispatch(
    control: RecordingControl,
    senders: &super::super::ManualWorkerSenders,
    source: &mut impl TrayCommandSource,
) {
    if senders.control_active_recording(control).is_err() {
        let _ = source.show_notice(TrayNoticeKind::ActionRejected);
    }
}
