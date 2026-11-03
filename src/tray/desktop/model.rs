//! Internal presentation DTOs. They do not extend the browser extension protocol.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum CaptureTarget {
    Region,
    Display,
    Window,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum CaptureQuality {
    Low,
    Balanced,
    High,
    Ultra,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub(crate) enum CaptureOptions {
    Screenshot {
        target: CaptureTarget,
    },
    Video {
        target: CaptureTarget,
        quality: CaptureQuality,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(
    tag = "command",
    rename_all = "lowercase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub(crate) enum CaptureRequest {
    Start {
        options: CaptureOptions,
        expected_epoch: u64,
    },
    Pause {
        expected_epoch: u64,
    },
    Resume {
        expected_epoch: u64,
    },
    Stop {
        expected_epoch: u64,
    },
    Discard {
        expected_epoch: u64,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum CaptureKind {
    Screenshot,
    Video,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum CapturePhase {
    Idle,
    Selecting,
    Starting,
    Recording,
    Paused,
    Saving,
    Complete,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum CaptureCommand {
    Start,
    Pause,
    Resume,
    Stop,
    Discard,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum CaptureOutcome {
    Saved,
    Discarded,
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "Shared capability contract; Windows capture currently requires no system permission prompt."
        )
    )]
    PermissionDenied,
    SaveFailed,
    CaptureFailed,
    Cancelled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Permission {
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "Shared capability contract; Windows capture currently requires no system permission prompt."
        )
    )]
    Granted,
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "Shared capability contract; Windows capture currently requires no system permission prompt."
        )
    )]
    Prompt,
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "Shared capability contract; Windows capture currently requires no system permission prompt."
        )
    )]
    Denied,
    NotRequired,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Availability {
    pub(crate) supported: bool,
    pub(crate) permission: Permission,
    pub(crate) source_available: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct CaptureTargets {
    pub(crate) region: Availability,
    pub(crate) display: Availability,
    pub(crate) window: Availability,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct CaptureCapabilities {
    pub(crate) screenshot: Availability,
    pub(crate) video: Availability,
    pub(crate) targets: CaptureTargets,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CaptureSnapshot {
    pub(crate) revision: u64,
    pub(crate) operation_epoch: u64,
    pub(crate) default_quality: CaptureQuality,
    pub(crate) setup_required: bool,
    pub(crate) phase: CapturePhase,
    pub(crate) kind: CaptureKind,
    pub(crate) elapsed_ms: u64,
    pub(crate) commands: Vec<CaptureCommand>,
    pub(crate) capabilities: CaptureCapabilities,
    pub(crate) outcome: Option<CaptureOutcome>,
}

pub(crate) use crate::settings::{AppearancePreferences as UiPreferences, UiLocale};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct PreferencesPatch {
    #[serde(default, deserialize_with = "present")]
    pub(crate) theme: Option<crate::settings::AppearanceThemePreference>,
    #[serde(default, deserialize_with = "present")]
    pub(crate) locale: Option<UiLocale>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum DesktopError {
    Unavailable,
    Busy,
    Rejected,
    UntrustedWindow,
    InvalidPreferences,
    InvalidLayout,
    SourceUnavailable,
    InvalidSettings,
    StaleSettings,
    RecordingBusy,
    HotkeyUnavailable,
    MicrophoneUnavailable,
}

/// Application coordination supplies validated state and admission. No OS handles cross this seam.
pub(crate) trait DesktopBackend: Send + Sync {
    fn snapshot(&self) -> Result<CaptureSnapshot, DesktopError>;
    fn command(&self, request: CaptureRequest) -> Result<(), DesktopError>;
    fn settings(
        &self,
        request: super::settings::SettingsRequest,
    ) -> Result<super::settings::SettingsSnapshot, DesktopError>;
    fn preferences(&self) -> Result<UiPreferences, DesktopError>;
    fn save_preferences(&self, patch: PreferencesPatch) -> Result<(), DesktopError>;
}

#[cfg(test)]
mod tests;

fn present<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

#[cfg(test)]
mod wire_tests {
    use super::{CaptureOutcome, DesktopError, Permission};

    #[test]
    fn capability_and_refusal_categories_keep_the_shared_web_wire_values()
    -> Result<(), serde_json::Error> {
        for (value, wire) in [
            (Permission::Granted, "granted"),
            (Permission::Prompt, "prompt"),
            (Permission::Denied, "denied"),
            (Permission::NotRequired, "not-required"),
        ] {
            assert_eq!(serde_json::to_value(value)?, wire);
        }
        assert_eq!(
            serde_json::to_value(CaptureOutcome::PermissionDenied)?,
            "permission-denied"
        );
        assert_eq!(
            serde_json::to_value(DesktopError::UntrustedWindow)?,
            "untrusted-window"
        );
        Ok(())
    }
}
