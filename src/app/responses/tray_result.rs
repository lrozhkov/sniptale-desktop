//! Extension tray action result side effects.

use crate::tray::TrayNoticeKind;

use super::effects::NativeHostSideEffects;

pub(super) fn handle_tray_action_result(accepted: bool, effects: &mut impl NativeHostSideEffects) {
    if !accepted {
        let _ = effects.show_tray_notice(TrayNoticeKind::ActionRejected);
    }
}
