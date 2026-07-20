//! Extension-intent event publication after first-party shell revalidation.

use crate::{
    app::resident::AgentEventHub,
    tray::{ShellActionId, TrayActionRegistry, TrayCommandSource, TrayNoticeKind},
};

use super::show_notice_if_not_queued;

pub(super) fn dispatch(
    shell_action: ShellActionId,
    registry: &TrayActionRegistry,
    connected: bool,
    events: &AgentEventHub,
    source: &mut impl TrayCommandSource,
) {
    let publish_result = events.publish_tray_extension_intent(registry, shell_action, connected);
    match publish_result {
        Ok(result) => show_notice_if_not_queued(source, result, false),
        Err(crate::tray::TrayActionError::Disconnected) => {
            let _ = source.show_notice(TrayNoticeKind::ExtensionDisconnected);
        }
        Err(_) => {
            let _ = source.show_notice(TrayNoticeKind::ActionRejected);
        }
    }
}
