//! Validated construction of Manual-mode profile candidates.

use std::collections::BTreeMap;

use super::model::{
    ManualAudio, ManualAudioSource, ManualDestination, ManualFps, ManualHardwarePreference,
    ManualHotkeyAction, ManualProfile, ManualScreenshot, ManualVideo, ManualVideoQuality,
};
use super::{ManualHotkeyChord, parse_manual_hotkey_chord};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ManualHotkeyUpdateError {
    Duplicate(ManualHotkeyAction),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ManualProfileDraft {
    destination_id: String,
    destination_path: String,
    screenshot_include_cursor: bool,
    video_quality: ManualVideoQuality,
    video_fps: ManualFps,
    video_bitrate_override_mbps: Option<u16>,
    video_hardware_preference: ManualHardwarePreference,
    video_include_cursor: bool,
    video_max_duration_seconds: u32,
    audio_source_mode: ManualAudioSource,
    microphone_device_id: Option<String>,
    audio_bitrate_kbps: u16,
    hotkeys: BTreeMap<String, String>,
}

/// Presentation-safe snapshot of a staged Manual profile.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ManualProfileDraftView {
    pub(crate) destination_path: String,
    pub(crate) screenshot_include_cursor: bool,
    pub(crate) video_quality: ManualVideoQuality,
    pub(crate) video_fps: ManualFps,
    pub(crate) video_bitrate_override_mbps: Option<u16>,
    pub(crate) video_hardware_preference: ManualHardwarePreference,
    pub(crate) video_include_cursor: bool,
    pub(crate) video_max_duration_seconds: u32,
    pub(crate) audio_source_mode: ManualAudioSource,
    pub(crate) microphone_device_id: Option<String>,
    pub(crate) audio_bitrate_kbps: u16,
    pub(crate) hotkeys: BTreeMap<String, String>,
}

impl ManualProfileDraft {
    #[must_use]
    pub(crate) fn with_defaults(destination_id: String, destination_path: String) -> Self {
        Self {
            destination_id,
            destination_path,
            screenshot_include_cursor: true,
            video_quality: ManualVideoQuality::High,
            video_fps: ManualFps::Fps30,
            video_bitrate_override_mbps: None,
            video_hardware_preference: ManualHardwarePreference::Prefer,
            video_include_cursor: true,
            video_max_duration_seconds: 3_600,
            audio_source_mode: ManualAudioSource::Mixed,
            microphone_device_id: None,
            audio_bitrate_kbps: 128,
            hotkeys: BTreeMap::new(),
        }
    }

    pub(crate) fn set_destination(&mut self, destination_id: String, destination_path: String) {
        self.destination_id = destination_id;
        self.destination_path = destination_path;
    }

    pub(crate) fn set_screenshot_include_cursor(&mut self, include_cursor: bool) {
        self.screenshot_include_cursor = include_cursor;
    }

    #[cfg(any(windows, test))]
    pub(crate) fn set_video_quality(&mut self, quality: ManualVideoQuality) {
        self.video_quality = quality;
    }

    #[cfg(any(windows, test))]
    pub(crate) fn set_video_fps(&mut self, fps: ManualFps) {
        self.video_fps = fps;
    }

    #[cfg(any(windows, test))]
    pub(crate) fn set_video_bitrate_override(&mut self, bitrate_override_mbps: Option<u16>) {
        self.video_bitrate_override_mbps = bitrate_override_mbps;
    }

    #[cfg(any(windows, test))]
    pub(crate) fn set_video_hardware_preference(
        &mut self,
        hardware_preference: ManualHardwarePreference,
    ) {
        self.video_hardware_preference = hardware_preference;
    }

    #[cfg(any(windows, test))]
    pub(crate) fn set_video_include_cursor(&mut self, include_cursor: bool) {
        self.video_include_cursor = include_cursor;
    }

    #[cfg(any(windows, test))]
    pub(crate) fn set_video_max_duration_seconds(&mut self, max_duration_seconds: u32) {
        self.video_max_duration_seconds = max_duration_seconds;
    }

    #[cfg(any(windows, test))]
    pub(crate) fn set_audio_source_mode(&mut self, source_mode: ManualAudioSource) {
        self.audio_source_mode = source_mode;
    }

    #[cfg(any(windows, test))]
    pub(crate) fn set_microphone_device_id(&mut self, microphone_device_id: Option<String>) {
        self.microphone_device_id = microphone_device_id;
    }

    #[cfg(any(windows, test))]
    pub(crate) fn set_audio_bitrate_kbps(&mut self, bitrate_kbps: u16) {
        self.audio_bitrate_kbps = bitrate_kbps;
    }

    pub(crate) fn set_hotkey(
        &mut self,
        action: ManualHotkeyAction,
        chord: Option<ManualHotkeyChord>,
    ) -> Result<(), ManualHotkeyUpdateError> {
        if let Some(chord) = chord {
            if let Some(conflict) = self.hotkeys.iter().find_map(|(existing_action, value)| {
                (existing_action != action.as_str()
                    && parse_manual_hotkey_chord(value) == Some(chord))
                .then(|| ManualHotkeyAction::from_id(existing_action))
                .flatten()
            }) {
                return Err(ManualHotkeyUpdateError::Duplicate(conflict));
            }
            self.hotkeys
                .insert(action.as_str().to_owned(), chord.canonical_label());
        } else {
            self.hotkeys.remove(action.as_str());
        }
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn set_video(
        &mut self,
        quality: ManualVideoQuality,
        fps: ManualFps,
        bitrate_override_mbps: Option<u16>,
        hardware_preference: ManualHardwarePreference,
        include_cursor: bool,
        max_duration_seconds: u32,
    ) {
        self.video_quality = quality;
        self.video_fps = fps;
        self.video_bitrate_override_mbps = bitrate_override_mbps;
        self.video_hardware_preference = hardware_preference;
        self.video_include_cursor = include_cursor;
        self.video_max_duration_seconds = max_duration_seconds;
    }

    #[cfg(test)]
    pub(crate) fn set_audio(
        &mut self,
        source_mode: ManualAudioSource,
        microphone_device_id: Option<String>,
        bitrate_kbps: u16,
    ) {
        self.audio_source_mode = source_mode;
        self.microphone_device_id = microphone_device_id;
        self.audio_bitrate_kbps = bitrate_kbps;
    }

    #[must_use]
    pub(crate) fn view(&self) -> ManualProfileDraftView {
        ManualProfileDraftView {
            destination_path: self.destination_path.clone(),
            screenshot_include_cursor: self.screenshot_include_cursor,
            video_quality: self.video_quality,
            video_fps: self.video_fps,
            video_bitrate_override_mbps: self.video_bitrate_override_mbps,
            video_hardware_preference: self.video_hardware_preference,
            video_include_cursor: self.video_include_cursor,
            video_max_duration_seconds: self.video_max_duration_seconds,
            audio_source_mode: self.audio_source_mode,
            microphone_device_id: self.microphone_device_id.clone(),
            audio_bitrate_kbps: self.audio_bitrate_kbps,
            hotkeys: self.hotkeys.clone(),
        }
    }

    #[must_use]
    pub(crate) fn from_profile(profile: &ManualProfile) -> Self {
        Self {
            destination_id: profile.destination.id.clone(),
            destination_path: profile.destination.path.clone(),
            screenshot_include_cursor: profile.screenshot.include_cursor,
            video_quality: profile.video.quality,
            video_fps: profile.video.fps,
            video_bitrate_override_mbps: profile.video.bitrate_override_mbps,
            video_hardware_preference: profile.video.hardware_preference,
            video_include_cursor: profile.video.include_cursor,
            video_max_duration_seconds: profile.video.max_duration_seconds,
            audio_source_mode: profile.audio.source_mode,
            microphone_device_id: profile.audio.microphone_device_id.clone(),
            audio_bitrate_kbps: profile.audio.bitrate_kbps,
            hotkeys: profile.hotkeys.clone(),
        }
    }

    #[must_use]
    pub(crate) fn into_profile(self) -> ManualProfile {
        ManualProfile {
            destination: ManualDestination {
                id: self.destination_id,
                path: self.destination_path,
            },
            screenshot: ManualScreenshot {
                include_cursor: self.screenshot_include_cursor,
            },
            video: ManualVideo {
                quality: self.video_quality,
                fps: self.video_fps,
                bitrate_override_mbps: self.video_bitrate_override_mbps,
                hardware_preference: self.video_hardware_preference,
                include_cursor: self.video_include_cursor,
                max_duration_seconds: self.video_max_duration_seconds,
            },
            audio: ManualAudio {
                source_mode: self.audio_source_mode,
                microphone_device_id: self.microphone_device_id,
                bitrate_kbps: self.audio_bitrate_kbps,
            },
            hotkeys: self.hotkeys,
        }
    }
}

#[cfg(test)]
#[path = "draft_tests.rs"]
mod tests;
