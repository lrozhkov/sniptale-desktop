//! Persisted Manual-mode profile shape and invariant validation.

#[cfg(any(windows, test))]
use std::path::PathBuf;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Component, Path},
};

use serde::{Deserialize, Serialize};
#[cfg(any(windows, test))]
use serde_json::Number;

use super::super::profile::LocalProfileError;
#[cfg(any(windows, test))]
use crate::protocol::{
    NativeRequestedQualitySettings,
    recording::{NativeAudioSourceMode, NativeVideoFrameRate, NativeVideoQuality},
};

const MAX_HOTKEYS: usize = 16;
const MAX_DESTINATION_PATH_BYTES: usize = 32 * 1024;
const MAX_HOTKEY_CHARS: usize = 64;
const MAX_DEVICE_ID_BYTES: usize = 128;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ManualHotkeyAction {
    ScreenshotScreen,
    ScreenshotActiveWindow,
    ScreenshotRegion,
    RecordingStartScreen,
    RecordingStartActiveWindow,
    RecordingStartRegion,
    RecordingPauseResume,
    RecordingStop,
}

impl ManualHotkeyAction {
    pub(crate) const ALL: [Self; 8] = [
        Self::ScreenshotScreen,
        Self::ScreenshotActiveWindow,
        Self::ScreenshotRegion,
        Self::RecordingStartScreen,
        Self::RecordingStartActiveWindow,
        Self::RecordingStartRegion,
        Self::RecordingPauseResume,
        Self::RecordingStop,
    ];

    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::ScreenshotScreen => "screenshot-screen",
            Self::ScreenshotActiveWindow => "screenshot-active-window",
            Self::ScreenshotRegion => "screenshot-region",
            Self::RecordingStartScreen => "recording-start-screen",
            Self::RecordingStartActiveWindow => "recording-start-active-window",
            Self::RecordingStartRegion => "recording-start-region",
            Self::RecordingPauseResume => "recording-pause-resume",
            Self::RecordingStop => "recording-stop",
        }
    }

    pub(crate) fn from_id(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|action| action.as_str() == value)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ManualProfile {
    pub(crate) destination: ManualDestination,
    pub(crate) screenshot: ManualScreenshot,
    pub(crate) video: ManualVideo,
    pub(crate) audio: ManualAudio,
    pub(crate) hotkeys: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ManualDestination {
    pub(crate) id: String,
    pub(crate) path: String,
}

#[cfg(any(windows, test))]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ManualDestinationSnapshot {
    pub(crate) id: String,
    pub(crate) root: PathBuf,
}

#[cfg(any(windows, test))]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ManualRecordingSettings {
    pub(crate) microphone_device_id: Option<String>,
    pub(crate) requested_quality: NativeRequestedQualitySettings,
    pub(crate) max_duration_ms: u64,
    pub(crate) include_cursor: bool,
}

impl ManualProfile {
    #[must_use]
    #[cfg(any(windows, test))]
    pub(crate) fn destination_snapshot(&self) -> ManualDestinationSnapshot {
        ManualDestinationSnapshot {
            id: self.destination.id.clone(),
            root: PathBuf::from(&self.destination.path),
        }
    }

    #[must_use]
    #[cfg(any(windows, test))]
    pub(crate) fn recording_settings(&self) -> ManualRecordingSettings {
        ManualRecordingSettings {
            microphone_device_id: self.audio.microphone_device_id.clone(),
            requested_quality: NativeRequestedQualitySettings {
                audio_bitrate_kbps: self.audio.bitrate_kbps,
                audio_source_mode: match self.audio.source_mode {
                    ManualAudioSource::Microphone => NativeAudioSourceMode::Microphone,
                    ManualAudioSource::System => NativeAudioSourceMode::System,
                    ManualAudioSource::Mixed => NativeAudioSourceMode::Mixed,
                },
                frame_rate: match self.video.fps {
                    ManualFps::Fps24 => NativeVideoFrameRate::Fps24,
                    ManualFps::Fps30 => NativeVideoFrameRate::Fps30,
                    ManualFps::Fps60 => NativeVideoFrameRate::Fps60,
                },
                quality: match self.video.quality {
                    ManualVideoQuality::Low => NativeVideoQuality::Low,
                    ManualVideoQuality::Medium => NativeVideoQuality::Medium,
                    ManualVideoQuality::High => NativeVideoQuality::High,
                    ManualVideoQuality::Ultra => NativeVideoQuality::Ultra,
                },
                video_bitrate_mbps_override: self.video.bitrate_override_mbps.map(Number::from),
            },
            max_duration_ms: u64::from(self.video.max_duration_seconds) * 1_000,
            include_cursor: self.video.include_cursor,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ManualScreenshot {
    pub(crate) include_cursor: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ManualVideo {
    pub(crate) quality: ManualVideoQuality,
    pub(crate) fps: ManualFps,
    pub(crate) bitrate_override_mbps: Option<u16>,
    pub(crate) hardware_preference: ManualHardwarePreference,
    pub(crate) include_cursor: bool,
    pub(crate) max_duration_seconds: u32,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ManualVideoQuality {
    Low,
    Medium,
    High,
    Ultra,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ManualFps {
    Fps24,
    Fps30,
    Fps60,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ManualHardwarePreference {
    Prefer,
    ForceSoftware,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ManualAudio {
    pub(crate) source_mode: ManualAudioSource,
    pub(crate) microphone_device_id: Option<String>,
    pub(crate) bitrate_kbps: u16,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ManualAudioSource {
    Microphone,
    System,
    Mixed,
}

pub(crate) fn validate_manual_profile(profile: &ManualProfile) -> Result<(), LocalProfileError> {
    let destination_path = Path::new(&profile.destination.path);
    if profile.destination.id.is_empty()
        || profile.destination.id.len() > MAX_DEVICE_ID_BYTES
        || profile.destination.path.is_empty()
        || profile.destination.path.len() > MAX_DESTINATION_PATH_BYTES
        || profile.destination.path.contains('\0')
        || profile.destination.path.starts_with("\\\\")
        || profile.destination.path.starts_with("//")
        || !destination_path.is_absolute()
        || destination_path
            .components()
            .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
    {
        return Err(LocalProfileError::InvalidProfile);
    }
    if !(96..=192).contains(&profile.audio.bitrate_kbps)
        || profile
            .audio
            .microphone_device_id
            .as_deref()
            .is_some_and(|id| id.is_empty() || id.len() > MAX_DEVICE_ID_BYTES)
        || !(1..=43_200).contains(&profile.video.max_duration_seconds)
        || profile
            .video
            .bitrate_override_mbps
            .is_some_and(|value| !(2..=80).contains(&value))
        || profile.hotkeys.len() > MAX_HOTKEYS
    {
        return Err(LocalProfileError::InvalidProfile);
    }
    let mut chords = BTreeSet::new();
    for (action, chord) in &profile.hotkeys {
        let parsed = super::hotkey::parse_manual_hotkey_chord(chord);
        if ManualHotkeyAction::from_id(action).is_none()
            || action.len() > MAX_DEVICE_ID_BYTES
            || chord.is_empty()
            || chord.chars().count() > MAX_HOTKEY_CHARS
            || parsed.is_none_or(|parsed| !chords.insert(parsed))
        {
            return Err(LocalProfileError::InvalidProfile);
        }
    }
    Ok(())
}

#[cfg(test)]
mod hotkey_action_tests {
    use super::ManualHotkeyAction;

    #[test]
    fn manual_hotkey_actions_have_stable_ids_and_user_labels() {
        assert_eq!(
            ManualHotkeyAction::ScreenshotScreen.as_str(),
            "screenshot-screen"
        );
        assert_eq!(
            ManualHotkeyAction::from_id("recording-stop"),
            Some(ManualHotkeyAction::RecordingStop)
        );
    }
}
