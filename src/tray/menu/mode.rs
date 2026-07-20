//! Native presentation of the staged operating-mode picker.

use super::{
    actions::{ShellActionId, TrayActionKind},
    model::{TrayMenuEntry, TrayMenuItem},
};
use crate::settings::OperatingMode;

/// Selecting another authority opens its staged settings flow, which remains the only place that
/// commits a mode transition.
pub(super) fn submenu(active_mode: OperatingMode, switch_enabled: bool) -> TrayMenuEntry {
    TrayMenuEntry::submenu(
        "operating-mode",
        "Mode",
        true,
        vec![
            entry(
                ShellActionId::ModeExtensionManaged,
                "Extension-managed",
                active_mode == OperatingMode::ExtensionManaged,
                switch_enabled,
            ),
            entry(
                ShellActionId::ModeManual,
                "Manual",
                active_mode == OperatingMode::Manual,
                switch_enabled,
            ),
        ],
    )
}

fn entry(id: ShellActionId, label: &str, active: bool, switch_enabled: bool) -> TrayMenuEntry {
    TrayMenuEntry::item(
        TrayMenuItem::configured(id.as_str(), label, TrayActionKind::OpenSettings)
            .with_enabled(!active && switch_enabled)
            .with_radio_check(active),
    )
}
