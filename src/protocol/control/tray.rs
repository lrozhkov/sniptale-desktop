use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum NativeTrayActionKind {
    OpenSettings,
    CaptureScreenshot,
    StartRecording,
    StopRecording,
    PauseRecording,
    ResumeRecording,
    OpenGallery,
    OpenVideoEditor,
}

/// Canonical v1 wire identifiers for first-party extension-authorized media intents.
///
/// Shell-owned controls intentionally do not have variants in this namespace.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ExtensionIntentActionId {
    ScreenshotScreen,
    ScreenshotActiveWindow,
    ScreenshotAllScreens,
    ScreenshotRegion,
    RecordingStartScreen,
    RecordingStartActiveWindow,
    RecordingStartRegion,
}

impl ExtensionIntentActionId {
    #[cfg(test)]
    #[must_use]
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::ScreenshotScreen => "screenshot-screen",
            Self::ScreenshotActiveWindow => "screenshot-active-window",
            Self::ScreenshotAllScreens => "screenshot-all-screens",
            Self::ScreenshotRegion => "screenshot-region",
            Self::RecordingStartScreen => "recording-start-screen",
            Self::RecordingStartActiveWindow => "recording-start-active-window",
            Self::RecordingStartRegion => "recording-start-region",
        }
    }

    #[must_use]
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "screenshot-screen" => Some(Self::ScreenshotScreen),
            "screenshot-active-window" => Some(Self::ScreenshotActiveWindow),
            "screenshot-all-screens" => Some(Self::ScreenshotAllScreens),
            "screenshot-region" => Some(Self::ScreenshotRegion),
            "recording-start-screen" => Some(Self::RecordingStartScreen),
            "recording-start-active-window" => Some(Self::RecordingStartActiveWindow),
            "recording-start-region" => Some(Self::RecordingStartRegion),
            _ => None,
        }
    }

    /// Maps either a canonical descriptor id or a currently shipped legacy id to the typed intent.
    #[must_use]
    pub(crate) fn from_descriptor(id: &str, kind: &NativeTrayActionKind) -> Option<Self> {
        let canonical = Self::parse(id);
        match (canonical, id, kind) {
            (Some(intent), _, _) if intent.matches_kind(kind) => Some(intent),
            (
                None,
                "capture-screen" | "capture-screenshot-screen",
                NativeTrayActionKind::CaptureScreenshot,
            ) => Some(Self::ScreenshotScreen),
            (None, "capture-screenshot-active-window", NativeTrayActionKind::CaptureScreenshot) => {
                Some(Self::ScreenshotActiveWindow)
            }
            (None, "capture-screenshot-all-screens", NativeTrayActionKind::CaptureScreenshot) => {
                Some(Self::ScreenshotAllScreens)
            }
            (None, "capture-screenshot-region", NativeTrayActionKind::CaptureScreenshot) => {
                Some(Self::ScreenshotRegion)
            }
            (
                None,
                "start-recording" | "start-recording-screen",
                NativeTrayActionKind::StartRecording,
            ) => Some(Self::RecordingStartScreen),
            (None, "start-recording-active-window", NativeTrayActionKind::StartRecording) => {
                Some(Self::RecordingStartActiveWindow)
            }
            (None, "start-recording-region", NativeTrayActionKind::StartRecording) => {
                Some(Self::RecordingStartRegion)
            }
            _ => None,
        }
    }

    #[must_use]
    pub(crate) const fn matches_kind(self, kind: &NativeTrayActionKind) -> bool {
        match self {
            Self::ScreenshotScreen
            | Self::ScreenshotActiveWindow
            | Self::ScreenshotAllScreens
            | Self::ScreenshotRegion => matches!(kind, NativeTrayActionKind::CaptureScreenshot),
            Self::RecordingStartScreen
            | Self::RecordingStartActiveWindow
            | Self::RecordingStartRegion => matches!(kind, NativeTrayActionKind::StartRecording),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeTrayActionDescriptor {
    pub(crate) enabled: bool,
    pub(crate) id: String,
    pub(crate) kind: NativeTrayActionKind,
    pub(crate) label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) shortcut_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) warning: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) offline_capable: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeTrayShortcutPriority {
    pub(crate) shortcut_labels: Vec<String>,
    pub(crate) when: NativeTrayShortcutPriorityWhen,
    pub(crate) winner: NativeTrayShortcutPriorityWinner,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum NativeTrayShortcutPriorityWhen {
    BrowserActive,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum NativeTrayShortcutPriorityWinner {
    Extension,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeTrayActionRegistry {
    pub(crate) actions: Vec<NativeTrayActionDescriptor>,
    pub(crate) revision: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) shortcut_priority: Option<NativeTrayShortcutPriority>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AppTrayActionRequestedMessage {
    pub(crate) protocol_version: u16,
    pub(crate) controller_lease_id: String,
    pub(crate) action_id: String,
    pub(crate) requested_at_epoch_ms: u64,
    pub(crate) invocation_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExtensionTrayActionResultMessage {
    pub(crate) protocol_version: u16,
    pub(crate) controller_lease_id: String,
    pub(crate) invocation_id: String,
    pub(crate) accepted: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) error: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum OpenSettingsSection {
    NativeApp,
    NativeHotkeys,
    NativeScreenshots,
    NativeVideo,
    NativeTelemetry,
    Permissions,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AppOpenSettingsRequestedMessage {
    pub(crate) protocol_version: u16,
    pub(crate) controller_lease_id: String,
    pub(crate) section: OpenSettingsSection,
    pub(crate) requested_at_epoch_ms: u64,
    pub(crate) invocation_id: String,
}

#[cfg(test)]
#[path = "tray/tests.rs"]
mod tests;
