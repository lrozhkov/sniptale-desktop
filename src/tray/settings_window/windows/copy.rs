//! Concise user-facing copy for the native settings presentation.

use crate::tray::settings_window::model::SettingsWindowView;

pub(super) fn title(_view: SettingsWindowView) -> &'static str {
    "Sniptale — Settings"
}

pub(super) fn headline(view: SettingsWindowView) -> &'static str {
    match view {
        SettingsWindowView::ManualDraft | SettingsWindowView::ExtensionManagedReadOnly => {
            "Desktop settings"
        }
        SettingsWindowView::ExtensionManagedDraft => "Switch to extension mode",
    }
}

pub(super) fn body(view: SettingsWindowView) -> &'static str {
    match view {
        SettingsWindowView::ManualDraft => "Capture locally, even when the browser is closed.",
        SettingsWindowView::ExtensionManagedReadOnly => {
            "Choose a local folder to enable capture and recording."
        }
        SettingsWindowView::ExtensionManagedDraft => {
            "New capture and recording requests will use the browser extension."
        }
    }
}

pub(super) fn landing_badge(view: SettingsWindowView) -> &'static str {
    match view {
        SettingsWindowView::ExtensionManagedReadOnly => "GET STARTED",
        SettingsWindowView::ExtensionManagedDraft => "MODE CHANGE",
        SettingsWindowView::ManualDraft => "DESKTOP",
    }
}

pub(super) fn landing_title(view: SettingsWindowView) -> &'static str {
    match view {
        SettingsWindowView::ExtensionManagedReadOnly => "Choose where to save",
        SettingsWindowView::ExtensionManagedDraft => "Hand control back to the extension?",
        SettingsWindowView::ManualDraft => "Desktop settings",
    }
}

pub(super) fn landing_body(view: SettingsWindowView) -> &'static str {
    match view {
        SettingsWindowView::ExtensionManagedReadOnly => {
            "Set a local folder before your first capture. You can then send results to a trusted browser or choose a destination after each capture."
        }
        SettingsWindowView::ExtensionManagedDraft => {
            "After you save, new capture and recording requests will use settings from the connected browser extension."
        }
        SettingsWindowView::ManualDraft => "",
    }
}

pub(super) fn landing_note(view: SettingsWindowView) -> &'static str {
    match view {
        SettingsWindowView::ExtensionManagedReadOnly => {
            "Your local folder keeps results safe when a browser is unavailable."
        }
        SettingsWindowView::ExtensionManagedDraft => {
            "Manual shortcuts are disabled while the extension owns settings."
        }
        SettingsWindowView::ManualDraft => "",
    }
}
