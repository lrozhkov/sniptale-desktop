//! Checked projection of first-party tray actions into accepted extension intents.

use super::{
    actions::{
        ShellActionId, TrayActionError, TrayActionKind, TrayActionRegistry, TrayActionRequest,
    },
    mode,
    model::{TrayMenu, TrayMenuEntry, TrayMenuItem, fallback_items, is_fallback_id, status},
};
use crate::protocol::ExtensionIntentActionId;
use crate::settings::OperatingMode;
use crate::tray::TrayRecordingStatus;

impl TrayActionRegistry {
    /// Reports whether an accepted, enabled registry entry authorizes this shell action.
    #[must_use]
    pub(crate) fn supports_extension_intent(&self, shell_action: ShellActionId) -> bool {
        self.extension_action(shell_action)
            .is_some_and(TrayMenuItem::enabled)
    }

    /// Revalidates a first-party action and returns its accepted wire-domain action identifier.
    pub(crate) fn request_extension_intent(
        &self,
        shell_action: ShellActionId,
        connected: bool,
        invocation_id: impl Into<String>,
        requested_at_epoch_ms: u64,
    ) -> Result<TrayActionRequest, TrayActionError> {
        let action = self
            .extension_action(shell_action)
            .ok_or(TrayActionError::UnknownAction)?;
        self.request_action(action.id(), connected, invocation_id, requested_at_epoch_ms)
    }

    fn extension_action(&self, shell_action: ShellActionId) -> Option<&TrayMenuItem> {
        let intent = shell_action.extension_intent()?;
        self.actions().iter().find(|action| {
            action.extension_intent() == Some(intent)
                && intent_matches_action_kind(intent, action.action_kind())
        })
    }
}

const fn intent_matches_action_kind(
    intent: ExtensionIntentActionId,
    kind: Option<TrayActionKind>,
) -> bool {
    match intent {
        ExtensionIntentActionId::ScreenshotScreen
        | ExtensionIntentActionId::ScreenshotActiveWindow
        | ExtensionIntentActionId::ScreenshotAllScreens
        | ExtensionIntentActionId::ScreenshotRegion => {
            matches!(kind, Some(TrayActionKind::CaptureScreenshot))
        }
        ExtensionIntentActionId::RecordingStartScreen
        | ExtensionIntentActionId::RecordingStartActiveWindow
        | ExtensionIntentActionId::RecordingStartRegion => {
            matches!(kind, Some(TrayActionKind::StartRecording))
        }
    }
}

/// Builds the extension-managed tree from native policy plus an accepted registry capability set.
#[must_use]
pub(super) fn build_extension_menu(
    registry: Option<&TrayActionRegistry>,
    connected: bool,
    recording: TrayRecordingStatus,
) -> TrayMenu {
    let mut entries = vec![TrayMenuEntry::item(status(
        "status",
        if connected {
            "Sniptale — Extension: connected"
        } else {
            "Sniptale — Extension: disconnected"
        },
    ))];
    entries.push(TrayMenuEntry::item(status(
        "extension-recording-status",
        recording_label(recording),
    )));
    entries.push(TrayMenuEntry::Separator);
    entries.push(submenu(
        "extension-screenshot",
        "Screenshot",
        screenshot_items(registry, connected, recording),
    ));
    entries.push(submenu(
        "extension-video",
        "Video",
        video_items(registry, connected, recording),
    ));
    if let Some(extra_items) = extension_extra_items(registry, connected) {
        entries.push(TrayMenuEntry::Separator);
        entries.push(TrayMenuEntry::submenu(
            "extension-actions",
            "Extension actions",
            true,
            extra_items.into_iter().map(TrayMenuEntry::item).collect(),
        ));
    }
    entries.push(TrayMenuEntry::Separator);
    entries.push(mode::submenu(
        OperatingMode::ExtensionManaged,
        matches!(recording, TrayRecordingStatus::Idle),
    ));
    entries.push(TrayMenuEntry::item(
        TrayMenuItem::configured(
            ShellActionId::OpenExtensionSettings.as_str(),
            "Open extension settings",
            TrayActionKind::OpenSettings,
        )
        .with_enabled(connected),
    ));
    entries.extend(fallback_items());
    TrayMenu::new(entries)
}

fn screenshot_items(
    registry: Option<&TrayActionRegistry>,
    connected: bool,
    recording: TrayRecordingStatus,
) -> Vec<TrayMenuItem> {
    vec![
        intent_item(
            ShellActionId::ScreenshotScreen,
            "Current screen",
            registry,
            connected,
            recording,
        ),
        intent_item(
            ShellActionId::ScreenshotActiveWindow,
            "Active window",
            registry,
            connected,
            recording,
        ),
        intent_item(
            ShellActionId::ScreenshotAllScreens,
            "All screens",
            registry,
            connected,
            recording,
        ),
        intent_item(
            ShellActionId::ScreenshotRegion,
            "Region…",
            registry,
            connected,
            recording,
        ),
    ]
}

fn recording_safety_items(recording: TrayRecordingStatus) -> Vec<TrayMenuItem> {
    let can_pause = matches!(recording, TrayRecordingStatus::Recording);
    let can_resume = matches!(recording, TrayRecordingStatus::Paused);
    let can_stop = matches!(
        recording,
        TrayRecordingStatus::Preparing
            | TrayRecordingStatus::Recording
            | TrayRecordingStatus::Paused
    );
    vec![
        TrayMenuItem::configured(
            ShellActionId::RecordingPause.as_str(),
            "Pause",
            TrayActionKind::PauseRecording,
        )
        .with_enabled(can_pause),
        TrayMenuItem::configured(
            ShellActionId::RecordingResume.as_str(),
            "Resume",
            TrayActionKind::ResumeRecording,
        )
        .with_enabled(can_resume),
        TrayMenuItem::configured(
            ShellActionId::RecordingStop.as_str(),
            "Stop",
            TrayActionKind::StopRecording,
        )
        .with_enabled(can_stop),
        TrayMenuItem::configured(
            ShellActionId::RecordingCancel.as_str(),
            "Cancel and discard",
            TrayActionKind::StopRecording,
        )
        .with_enabled(can_stop),
    ]
}

fn video_items(
    registry: Option<&TrayActionRegistry>,
    connected: bool,
    recording: TrayRecordingStatus,
) -> Vec<TrayMenuItem> {
    let mut items = vec![
        intent_item(
            ShellActionId::RecordingStartScreen,
            "Start current screen",
            registry,
            connected,
            recording,
        ),
        intent_item(
            ShellActionId::RecordingStartActiveWindow,
            "Start active window",
            registry,
            connected,
            recording,
        ),
        intent_item(
            ShellActionId::RecordingStartRegion,
            "Start region…",
            registry,
            connected,
            recording,
        ),
    ];
    items.extend(recording_safety_items(recording));
    items
}

fn intent_item(
    shell_action: ShellActionId,
    label: &str,
    registry: Option<&TrayActionRegistry>,
    connected: bool,
    recording: TrayRecordingStatus,
) -> TrayMenuItem {
    let kind = shell_action
        .extension_intent()
        .map_or(TrayActionKind::CaptureScreenshot, intent_kind);
    let enabled = matches!(recording, TrayRecordingStatus::Idle)
        && connected
        && registry.is_some_and(|registry| registry.supports_extension_intent(shell_action));
    TrayMenuItem::configured(shell_action.as_str(), label, kind).with_enabled(enabled)
}

const fn recording_label(recording: TrayRecordingStatus) -> &'static str {
    match recording {
        TrayRecordingStatus::Idle => "Idle",
        TrayRecordingStatus::Preparing => "Preparing",
        TrayRecordingStatus::Recording => "Recording",
        TrayRecordingStatus::Paused => "Paused",
        TrayRecordingStatus::Finalizing => "Finalizing",
    }
}

const fn intent_kind(intent: ExtensionIntentActionId) -> TrayActionKind {
    match intent {
        ExtensionIntentActionId::ScreenshotScreen
        | ExtensionIntentActionId::ScreenshotActiveWindow
        | ExtensionIntentActionId::ScreenshotAllScreens
        | ExtensionIntentActionId::ScreenshotRegion => TrayActionKind::CaptureScreenshot,
        ExtensionIntentActionId::RecordingStartScreen
        | ExtensionIntentActionId::RecordingStartActiveWindow
        | ExtensionIntentActionId::RecordingStartRegion => TrayActionKind::StartRecording,
    }
}

fn extension_extra_items(
    registry: Option<&TrayActionRegistry>,
    connected: bool,
) -> Option<Vec<TrayMenuItem>> {
    let entries = registry?
        .actions()
        .iter()
        .filter(|item| {
            item.extension_intent().is_none()
                && !is_fallback_id(item.id())
                && item.enabled()
                && (connected || item.offline_capable())
        })
        .cloned()
        .collect::<Vec<_>>();
    (!entries.is_empty()).then_some(entries)
}

fn submenu(id: &'static str, label: &'static str, items: Vec<TrayMenuItem>) -> TrayMenuEntry {
    TrayMenuEntry::submenu(
        id,
        label,
        true,
        items.into_iter().map(TrayMenuEntry::item).collect(),
    )
}
