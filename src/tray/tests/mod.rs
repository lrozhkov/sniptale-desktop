mod actions;
mod browser;
mod consent;
mod icon;
mod menu;

use crate::{
    protocol::{
        ExtensionIntentActionId, NativeTrayActionDescriptor, NativeTrayActionKind,
        NativeTrayActionRegistry,
    },
    tray::{
        TrayActionKind, TrayActionRegistry, TrayConnectionStatus, TrayIconState,
        TrayRecordingStatus, TrayStartupStatus,
    },
};

use super::menu::model::TrayMenuItem;

fn registry() -> TrayActionRegistry {
    TrayActionRegistry::new(vec![
        TrayMenuItem::configured(
            "screenshot-screen",
            "Capture",
            TrayActionKind::CaptureScreenshot,
        )
        .with_enabled(true)
        .with_extension_intent(Some(ExtensionIntentActionId::ScreenshotScreen)),
        TrayMenuItem::configured("open-gallery", "Gallery", TrayActionKind::OpenGallery)
            .with_enabled(true)
            .with_offline_capable(true),
        TrayMenuItem::configured("disabled", "Disabled", TrayActionKind::StartRecording)
            .with_enabled(false),
    ])
}

fn registry_with_fallback_collision() -> TrayActionRegistry {
    TrayActionRegistry::new(vec![
        TrayMenuItem::configured(
            "open-settings",
            "Extension Settings",
            TrayActionKind::OpenSettings,
        )
        .with_enabled(true)
        .with_shortcut_label(Some("Ctrl+,".to_owned())),
        TrayMenuItem::configured(
            "screenshot-screen",
            "Capture",
            TrayActionKind::CaptureScreenshot,
        )
        .with_enabled(true)
        .with_extension_intent(Some(ExtensionIntentActionId::ScreenshotScreen)),
    ])
}

fn native_registry() -> NativeTrayActionRegistry {
    NativeTrayActionRegistry {
        revision: "tray-revision-1".to_owned(),
        shortcut_priority: None,
        actions: vec![
            NativeTrayActionDescriptor {
                enabled: true,
                id: "screenshot-screen".to_owned(),
                kind: NativeTrayActionKind::CaptureScreenshot,
                label: "Capture".to_owned(),
                shortcut_label: Some("Ctrl+Shift+S".to_owned()),
                warning: Some("Requires browser connection".to_owned()),
                offline_capable: Some(false),
            },
            NativeTrayActionDescriptor {
                enabled: true,
                id: "open-gallery".to_owned(),
                kind: NativeTrayActionKind::OpenGallery,
                label: "Gallery".to_owned(),
                shortcut_label: None,
                warning: None,
                offline_capable: Some(true),
            },
        ],
    }
}

fn icon_state() -> TrayIconState {
    TrayIconState {
        connection: TrayConnectionStatus::Connected,
        recording: TrayRecordingStatus::Idle,
        startup_status: TrayStartupStatus::Operational,
    }
}
