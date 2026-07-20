//! Concise user-facing copy for the native settings presentation.

use crate::tray::settings_window::model::SettingsWindowView;

pub(super) fn title(view: SettingsWindowView) -> &'static str {
    match view {
        SettingsWindowView::ManualDraft => "Sniptale — Manual settings",
        SettingsWindowView::ExtensionManagedReadOnly
        | SettingsWindowView::ExtensionManagedDraft => "Sniptale — Settings",
    }
}

pub(super) fn headline(view: SettingsWindowView) -> &'static str {
    match view {
        SettingsWindowView::ManualDraft => "Manual settings",
        SettingsWindowView::ExtensionManagedReadOnly => "Extension-managed settings",
        SettingsWindowView::ExtensionManagedDraft => "Switch to extension mode",
    }
}

pub(super) fn body(view: SettingsWindowView) -> &'static str {
    match view {
        SettingsWindowView::ManualDraft => "Capture locally, even when the browser is closed.",
        SettingsWindowView::ExtensionManagedReadOnly => {
            "The browser extension currently owns capture and recording settings."
        }
        SettingsWindowView::ExtensionManagedDraft => {
            "New capture and recording requests will use the browser extension."
        }
    }
}

pub(super) fn landing_badge(view: SettingsWindowView) -> &'static str {
    match view {
        SettingsWindowView::ExtensionManagedReadOnly => "EXTENSION MODE",
        SettingsWindowView::ExtensionManagedDraft => "MODE CHANGE",
        SettingsWindowView::ManualDraft => "MANUAL MODE",
    }
}

pub(super) fn landing_title(view: SettingsWindowView) -> &'static str {
    match view {
        SettingsWindowView::ExtensionManagedReadOnly => "Your browser is in control",
        SettingsWindowView::ExtensionManagedDraft => "Hand control back to the extension?",
        SettingsWindowView::ManualDraft => "Manual settings",
    }
}

pub(super) fn landing_body(view: SettingsWindowView) -> &'static str {
    match view {
        SettingsWindowView::ExtensionManagedReadOnly => {
            "Capture, recording, delivery, and shortcuts follow the Sniptale extension settings. Set up Manual mode only when you want the desktop app to work independently."
        }
        SettingsWindowView::ExtensionManagedDraft => {
            "After you save, new capture and recording requests will use settings from the connected browser extension."
        }
        SettingsWindowView::ManualDraft => "",
    }
}

pub(super) fn landing_note(view: SettingsWindowView) -> &'static str {
    match view {
        SettingsWindowView::ExtensionManagedReadOnly => "You can switch back at any time.",
        SettingsWindowView::ExtensionManagedDraft => {
            "Manual shortcuts are disabled while the extension owns settings."
        }
        SettingsWindowView::ManualDraft => "",
    }
}
