//! Shell tray icon, fallback menu, extension-configured menu, and action requests.

#[cfg(any(windows, test))]
pub(crate) mod consent;
mod icon;
mod menu;
#[cfg(any(windows, test))]
pub(crate) mod runtime;
#[cfg(any(windows, test))]
pub(crate) mod settings_window;
#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) use crate::protocol::ExtensionIntentActionId;
#[cfg(any(windows, test))]
pub(crate) use consent::prompt::{
    ConnectionConsentPrompt, ConnectionConsentPromptRequest, ConnectionConsentPromptResult,
};
#[cfg(test)]
pub(crate) use icon::state::TrayIconVariant;
pub(crate) use icon::state::TrayNoticeKind;
#[cfg(any(windows, test))]
pub(crate) use icon::state::{
    TrayConnectionStatus, TrayIconState, TrayRecordingStatus, TrayStartupStatus,
    manual_tray_icon_tooltip, tray_icon_tooltip, tray_icon_variant,
};
#[cfg(any(windows, test))]
pub(crate) use menu::actions::ShellActionId;
#[cfg(any(windows, test))]
pub(crate) use menu::actions::TrayActionKind;
#[cfg(any(windows, test))]
pub(crate) use menu::actions::{TrayActionError, TrayActionRequest};
pub(crate) use menu::actions::{TrayActionRegistry, registry_from_action_catalog};
#[cfg(any(windows, test))]
pub(crate) use menu::manual::build_manual_menu;
#[cfg(any(windows, test))]
pub(crate) use menu::model::build_menu;
#[cfg(test)]
pub(crate) use menu::model::{TrayMenuEntry, TrayMenuItem};
#[cfg(any(windows, test))]
pub(crate) use menu::render::{TrayMenuCommand, plan_menu_render};
#[cfg(test)]
pub(crate) use menu::render::{TrayMenuRenderEntry, TrayMenuRenderPlan};
#[cfg(any(windows, test))]
pub(crate) use runtime::browser::plan_open_settings_fallback;
#[cfg(all(test, not(windows)))]
pub(crate) use runtime::source::NoopTrayCommandSource;
#[cfg(any(windows, test))]
pub(crate) use runtime::source::TrayCommandSource;
#[cfg(any(windows, test))]
pub(crate) use runtime::source::TrayRuntimeError;
#[cfg(any(windows, test))]
pub(crate) use runtime::source::TrayRuntimeEvent;
