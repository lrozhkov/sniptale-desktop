//! Fallback and extension-configured tray menu owner.

#[cfg(any(windows, test))]
use super::actions::ShellActionId;
use super::actions::TrayActionKind;
#[cfg(any(windows, test))]
use super::actions::TrayActionRegistry;
use crate::protocol::ExtensionIntentActionId;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TrayMenuItem {
    id: String,
    label: String,
    action_kind: Option<TrayActionKind>,
    shortcut_label: Option<String>,
    warning: Option<String>,
    enabled: bool,
    offline_capable: bool,
    fallback: bool,
    extension_intent: Option<ExtensionIntentActionId>,
    mark: TrayMenuMark,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum TrayMenuMark {
    #[default]
    None,
    #[cfg(any(windows, test))]
    Radio { checked: bool },
}

/// Presentation-only tree for a native popup menu.
///
/// The menu owner deliberately keeps hierarchy separate from action authorization: nested entries
/// only describe shell presentation, while actionable leaves still carry the same typed action
/// information used by the dispatcher.
#[cfg(any(windows, test))]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TrayMenu {
    entries: Vec<TrayMenuEntry>,
}

#[cfg(any(windows, test))]
impl TrayMenu {
    #[must_use]
    pub(crate) fn new(entries: Vec<TrayMenuEntry>) -> Self {
        Self { entries }
    }

    #[must_use]
    pub(crate) fn entries(&self) -> &[TrayMenuEntry] {
        &self.entries
    }
}

/// One native popup-menu entry. A separator and submenu never receive a command id.
#[cfg(any(windows, test))]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TrayMenuEntry {
    Item(TrayMenuItem),
    Separator,
    Submenu {
        id: &'static str,
        label: &'static str,
        enabled: bool,
        entries: Vec<TrayMenuEntry>,
    },
}

#[cfg(any(windows, test))]
impl TrayMenuEntry {
    #[must_use]
    pub(crate) fn item(item: TrayMenuItem) -> Self {
        Self::Item(item)
    }

    #[must_use]
    pub(crate) fn submenu(
        id: &'static str,
        label: &'static str,
        enabled: bool,
        entries: Vec<TrayMenuEntry>,
    ) -> Self {
        Self::Submenu {
            id,
            label,
            enabled,
            entries,
        }
    }
}

impl TrayMenuItem {
    #[must_use]
    pub(crate) fn configured(
        id: impl Into<String>,
        label: impl Into<String>,
        kind: TrayActionKind,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            action_kind: Some(kind),
            shortcut_label: None,
            warning: None,
            enabled: true,
            offline_capable: false,
            fallback: false,
            extension_intent: None,
            mark: TrayMenuMark::None,
        }
    }

    #[must_use]
    pub(crate) fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    #[must_use]
    pub(crate) fn with_offline_capable(mut self, offline_capable: bool) -> Self {
        self.offline_capable = offline_capable;
        self
    }

    #[must_use]
    pub(crate) fn with_shortcut_label(mut self, shortcut_label: Option<String>) -> Self {
        self.shortcut_label = shortcut_label;
        self
    }

    #[must_use]
    pub(crate) fn with_warning(mut self, warning: Option<String>) -> Self {
        self.warning = warning;
        self
    }

    #[must_use]
    pub(crate) fn with_extension_intent(
        mut self,
        extension_intent: Option<ExtensionIntentActionId>,
    ) -> Self {
        self.extension_intent = extension_intent;
        self
    }

    #[must_use]
    #[cfg(any(windows, test))]
    pub(crate) fn with_radio_check(mut self, checked: bool) -> Self {
        self.mark = TrayMenuMark::Radio { checked };
        self
    }

    #[must_use]
    #[cfg(any(windows, test))]
    pub(crate) fn id(&self) -> &str {
        &self.id
    }

    #[must_use]
    #[cfg(any(windows, test))]
    pub(crate) fn label(&self) -> &str {
        &self.label
    }

    #[must_use]
    #[cfg(any(windows, test))]
    pub(crate) const fn action_kind(&self) -> Option<TrayActionKind> {
        self.action_kind
    }

    #[must_use]
    #[cfg(any(windows, test))]
    pub(crate) const fn enabled(&self) -> bool {
        self.enabled
    }

    #[must_use]
    #[cfg(any(windows, test))]
    pub(crate) const fn offline_capable(&self) -> bool {
        self.offline_capable
    }

    #[must_use]
    #[cfg(any(windows, test))]
    pub(crate) fn shortcut_label(&self) -> Option<&str> {
        self.shortcut_label.as_deref()
    }

    #[must_use]
    #[cfg(any(windows, test))]
    pub(crate) fn warning(&self) -> Option<&str> {
        self.warning.as_deref()
    }

    #[must_use]
    #[cfg(any(windows, test))]
    pub(crate) const fn extension_intent(&self) -> Option<ExtensionIntentActionId> {
        self.extension_intent
    }

    #[must_use]
    #[cfg(any(windows, test))]
    pub(crate) const fn checked(&self) -> bool {
        matches!(self.mark, TrayMenuMark::Radio { checked: true })
    }

    #[must_use]
    #[cfg(any(windows, test))]
    pub(crate) const fn radio(&self) -> bool {
        matches!(self.mark, TrayMenuMark::Radio { .. })
    }
}

#[cfg(any(windows, test))]
#[must_use]
pub(crate) fn build_menu(
    registry: Option<&TrayActionRegistry>,
    connected: bool,
    recording: crate::tray::TrayRecordingStatus,
) -> TrayMenu {
    super::extension::build_extension_menu(registry, connected, recording)
}

#[cfg(any(windows, test))]
pub(super) fn fallback_items() -> Vec<TrayMenuEntry> {
    vec![
        TrayMenuEntry::item(fallback(
            ShellActionId::OpenSettings.as_str(),
            "Open Sniptale Settings",
            TrayActionKind::OpenSettings,
        )),
        TrayMenuEntry::Separator,
        TrayMenuEntry::item(fallback(
            ShellActionId::Quit.as_str(),
            "Exit Sniptale",
            TrayActionKind::Quit,
        )),
    ]
}

#[cfg(any(windows, test))]
fn fallback(id: &str, label: &str, kind: TrayActionKind) -> TrayMenuItem {
    TrayMenuItem {
        id: id.to_owned(),
        label: label.to_owned(),
        action_kind: Some(kind),
        shortcut_label: None,
        warning: None,
        enabled: true,
        offline_capable: true,
        fallback: true,
        extension_intent: None,
        mark: TrayMenuMark::None,
    }
}

#[cfg(any(windows, test))]
pub(super) fn status(id: &str, label: &str) -> TrayMenuItem {
    TrayMenuItem {
        id: id.to_owned(),
        label: label.to_owned(),
        action_kind: None,
        shortcut_label: None,
        warning: None,
        enabled: false,
        offline_capable: true,
        fallback: true,
        extension_intent: None,
        mark: TrayMenuMark::None,
    }
}

#[cfg(any(windows, test))]
pub(super) fn is_fallback_id(id: &str) -> bool {
    matches!(id, "status" | "open-settings" | "reconnect" | "quit")
}
