//! Tray action invocation request owner.

#[cfg(any(windows, test))]
use std::{error::Error, fmt};

use super::model::TrayMenuItem;
#[cfg(any(windows, test))]
use crate::protocol::ExtensionIntentActionId;
use crate::{protocol::NativeTrayActionKind, settings::ActionCatalog};

/// Native-owned identifiers are deliberately distinct from extension intent identifiers.
#[cfg(any(windows, test))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ShellActionId {
    ScreenshotScreen,
    ScreenshotActiveWindow,
    ScreenshotAllScreens,
    ScreenshotRegion,
    RecordingStartScreen,
    RecordingStartActiveWindow,
    RecordingStartRegion,
    RecordingPause,
    RecordingResume,
    RecordingStop,
    RecordingCancel,
    OpenSettings,
    ModeExtensionManaged,
    ModeManual,
    OpenSaveFolder,
    OpenExtensionSettings,
    RetryPendingSaves,
    DiscardPendingSaves,
    Quit,
}

#[cfg(any(windows, test))]
impl ShellActionId {
    const ALL: [Self; 19] = [
        Self::ScreenshotScreen,
        Self::ScreenshotActiveWindow,
        Self::ScreenshotAllScreens,
        Self::ScreenshotRegion,
        Self::RecordingStartScreen,
        Self::RecordingStartActiveWindow,
        Self::RecordingStartRegion,
        Self::RecordingPause,
        Self::RecordingResume,
        Self::RecordingStop,
        Self::RecordingCancel,
        Self::OpenSettings,
        Self::ModeExtensionManaged,
        Self::ModeManual,
        Self::OpenSaveFolder,
        Self::OpenExtensionSettings,
        Self::RetryPendingSaves,
        Self::DiscardPendingSaves,
        Self::Quit,
    ];

    pub(crate) fn as_str(self) -> &'static str {
        debug_assert_eq!(Self::ALL.len(), 19);
        match self {
            Self::ScreenshotScreen => "screenshot-screen",
            Self::ScreenshotActiveWindow => "screenshot-active-window",
            Self::ScreenshotAllScreens => "screenshot-all-screens",
            Self::ScreenshotRegion => "screenshot-region",
            Self::RecordingStartScreen => "recording-start-screen",
            Self::RecordingStartActiveWindow => "recording-start-active-window",
            Self::RecordingStartRegion => "recording-start-region",
            Self::RecordingPause => "recording-pause",
            Self::RecordingResume => "recording-resume",
            Self::RecordingStop => "recording-stop",
            Self::RecordingCancel => "recording-cancel",
            Self::OpenSettings => "open-settings",
            Self::ModeExtensionManaged => "mode-extension-managed",
            Self::ModeManual => "mode-manual",
            Self::OpenSaveFolder => "open-save-folder",
            Self::OpenExtensionSettings => "open-extension-settings",
            Self::RetryPendingSaves => "retry-pending-saves",
            Self::DiscardPendingSaves => "discard-pending-saves",
            Self::Quit => "quit",
        }
    }

    #[must_use]
    pub(crate) fn from_extension_action_id(value: &str) -> Option<Self> {
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

    /// Returns the explicit, checked wire-domain projection for an extension-authorized action.
    ///
    /// All other shell actions are handled locally and intentionally have no wire projection.
    pub(crate) const fn extension_intent(self) -> Option<ExtensionIntentActionId> {
        match self {
            Self::ScreenshotScreen => Some(ExtensionIntentActionId::ScreenshotScreen),
            Self::ScreenshotActiveWindow => Some(ExtensionIntentActionId::ScreenshotActiveWindow),
            Self::ScreenshotAllScreens => Some(ExtensionIntentActionId::ScreenshotAllScreens),
            Self::ScreenshotRegion => Some(ExtensionIntentActionId::ScreenshotRegion),
            Self::RecordingStartScreen => Some(ExtensionIntentActionId::RecordingStartScreen),
            Self::RecordingStartActiveWindow => {
                Some(ExtensionIntentActionId::RecordingStartActiveWindow)
            }
            Self::RecordingStartRegion => Some(ExtensionIntentActionId::RecordingStartRegion),
            Self::RecordingPause
            | Self::RecordingResume
            | Self::RecordingStop
            | Self::RecordingCancel
            | Self::OpenSettings
            | Self::ModeExtensionManaged
            | Self::ModeManual
            | Self::OpenSaveFolder
            | Self::OpenExtensionSettings
            | Self::RetryPendingSaves
            | Self::DiscardPendingSaves
            | Self::Quit => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TrayActionKind {
    OpenSettings,
    CaptureScreenshot,
    StartRecording,
    StopRecording,
    PauseRecording,
    ResumeRecording,
    OpenGallery,
    OpenVideoEditor,
    #[cfg(any(windows, test))]
    OpenSaveFolder,
    #[cfg(any(windows, test))]
    Quit,
}

#[cfg(any(windows, test))]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TrayActionError {
    UnknownAction,
    Disabled,
    Disconnected,
    InvalidInvocation,
}

#[cfg(any(windows, test))]
impl fmt::Display for TrayActionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownAction => formatter.write_str("unknown tray action"),
            Self::Disabled => formatter.write_str("tray action is disabled"),
            Self::Disconnected => formatter.write_str("tray action requires browser connection"),
            Self::InvalidInvocation => formatter.write_str("invalid tray action invocation"),
        }
    }
}

#[cfg(any(windows, test))]
impl Error for TrayActionError {}

#[cfg(any(windows, test))]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TrayActionRequest {
    pub(crate) action_id: String,
    pub(crate) invocation_id: String,
    pub(crate) requested_at_epoch_ms: u64,
    pub(crate) kind: TrayActionKind,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct TrayActionRegistry {
    actions: Vec<TrayMenuItem>,
}

impl TrayActionRegistry {
    #[must_use]
    pub(crate) fn new(actions: Vec<TrayMenuItem>) -> Self {
        Self { actions }
    }

    #[must_use]
    #[cfg(any(windows, test))]
    pub(crate) fn actions(&self) -> &[TrayMenuItem] {
        &self.actions
    }

    #[cfg(any(windows, test))]
    pub(crate) fn request_action(
        &self,
        action_id: &str,
        connected: bool,
        invocation_id: impl Into<String>,
        requested_at_epoch_ms: u64,
    ) -> Result<TrayActionRequest, TrayActionError> {
        let action = self
            .actions
            .iter()
            .find(|candidate| candidate.id() == action_id)
            .ok_or(TrayActionError::UnknownAction)?;
        if !action.enabled() {
            return Err(TrayActionError::Disabled);
        }
        if !connected && !action.offline_capable() {
            return Err(TrayActionError::Disconnected);
        }
        let invocation_id = invocation_id.into();
        if invocation_id.is_empty() || requested_at_epoch_ms == 0 {
            return Err(TrayActionError::InvalidInvocation);
        }
        Ok(TrayActionRequest {
            action_id: action.id().to_owned(),
            invocation_id,
            requested_at_epoch_ms,
            kind: action.action_kind().ok_or(TrayActionError::Disabled)?,
        })
    }
}

#[must_use]
pub(crate) fn registry_from_action_catalog(catalog: &ActionCatalog) -> TrayActionRegistry {
    let actions = catalog
        .entries()
        .iter()
        .map(|entry| {
            TrayMenuItem::configured(
                entry.id.clone(),
                entry.label.clone(),
                tray_action_kind(&entry.kind),
            )
            .with_enabled(entry.enabled)
            .with_offline_capable(entry.offline_capable)
            .with_shortcut_label(entry.shortcut_label.clone())
            .with_warning(entry.warning.clone())
            .with_extension_intent(entry.extension_intent)
        })
        .collect();
    TrayActionRegistry::new(actions)
}

const fn tray_action_kind(kind: &NativeTrayActionKind) -> TrayActionKind {
    match kind {
        NativeTrayActionKind::OpenSettings => TrayActionKind::OpenSettings,
        NativeTrayActionKind::CaptureScreenshot => TrayActionKind::CaptureScreenshot,
        NativeTrayActionKind::StartRecording => TrayActionKind::StartRecording,
        NativeTrayActionKind::StopRecording => TrayActionKind::StopRecording,
        NativeTrayActionKind::PauseRecording => TrayActionKind::PauseRecording,
        NativeTrayActionKind::ResumeRecording => TrayActionKind::ResumeRecording,
        NativeTrayActionKind::OpenGallery => TrayActionKind::OpenGallery,
        NativeTrayActionKind::OpenVideoEditor => TrayActionKind::OpenVideoEditor,
    }
}
