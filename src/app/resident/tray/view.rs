//! Typed tray-worker view lifecycle values.

use crate::tray::TrayStartupStatus;

use super::super::AgentStartupStatus;

pub(super) const fn tray_startup_status(status: AgentStartupStatus) -> TrayStartupStatus {
    match status {
        AgentStartupStatus::Operational => TrayStartupStatus::Operational,
        AgentStartupStatus::RepairRequired => TrayStartupStatus::RepairRequired,
        AgentStartupStatus::UnsupportedCacheSchema => TrayStartupStatus::UnsupportedCacheSchema,
        AgentStartupStatus::RepairRequiredAndUnsupportedCacheSchema => {
            TrayStartupStatus::RepairRequiredAndUnsupportedCacheSchema
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum TrayWorkerControl {
    Continue,
    Stop,
}

#[cfg(test)]
pub(super) fn command_action_ids(commands: &[crate::tray::TrayMenuCommand]) -> Vec<&str> {
    commands
        .iter()
        .map(|command| command.action_id.as_str())
        .collect()
}
